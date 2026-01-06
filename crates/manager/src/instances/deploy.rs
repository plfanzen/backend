// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

use compose_spec::Resource;
use k8s_openapi::api::{apps::v1::Deployment, core::v1::PersistentVolumeClaim};
use kube::{Api, Client};

use crate::repo::challenges::compose::{
    service::{AsDeployment, AsExternalService, AsIngress, AsService, ComposeServiceError},
    volume::{AsPvc, default_size_pvc},
};

pub async fn deploy_challenge(
    kube_client: &Client,
    challenge_ns: &str,
    challenge: compose_spec::Compose,
    exposed_domain: &str,
    working_dir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let (deployments, svcs, ingressroutes, ingressroutestcp): (Vec<_>, Vec<_>, Vec<_>, Vec<_>) =
        challenge.services.into_iter().try_fold(
            (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
            |(mut deployments, mut svcs, mut ingressroutes, mut ingressroutestcp),
             (svc_id, svc)|
             -> Result<_, ComposeServiceError> {
                deployments.push(svc.as_deployment(svc_id.to_string(), working_dir));
                svcs.push(svc.as_internal_svc(svc_id.to_string()));
                if let Some(external_svc) = svc.as_proxied_svc(svc_id.to_string())? {
                    svcs.push(external_svc);
                }
                if let Some(ir) =
                    svc.as_http_ingress(svc_id.to_string(), challenge_ns, exposed_domain)?
                {
                    ingressroutes.push(ir);
                }
                if let Some(irtcp) =
                    svc.as_tcp_ingress(svc_id.to_string(), challenge_ns, exposed_domain)?
                {
                    ingressroutestcp.push(irtcp);
                }
                Ok((deployments, svcs, ingressroutes, ingressroutestcp))
            },
        )?;

    let pvcs = challenge
        .volumes
        .into_iter()
        .map(|(vol_id, vol)| match vol {
            Some(Resource::External { .. }) => Err(()),
            Some(Resource::Compose(volume)) => Ok(volume.as_pvc(vol_id.to_string())),
            None => Ok(default_size_pvc(vol_id.to_string())),
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ComposeServiceError::ExternalVolume)?;

    let deployment_api: Api<Deployment> = Api::namespaced(kube_client.clone(), challenge_ns);
    for deployment in deployments {
        let deployment = deployment?;
        deployment_api
            .create(&Default::default(), &deployment)
            .await?;
    }
    let service_api: Api<k8s_openapi::api::core::v1::Service> =
        Api::namespaced(deployment_api.into_client(), challenge_ns);
    for service in svcs {
        service_api.create(&Default::default(), &service).await?;
    }

    let ingressroute_api: Api<k8s_crds_traefik::IngressRoute> =
        Api::namespaced(service_api.into_client(), challenge_ns);
    for ingressroute in ingressroutes {
        ingressroute_api
            .create(&Default::default(), &ingressroute)
            .await?;
    }
    let ingressroutetcp_api: Api<k8s_crds_traefik::IngressRouteTCP> =
        Api::namespaced(ingressroute_api.into_client(), challenge_ns);
    for ingressroutetcp in ingressroutestcp {
        ingressroutetcp_api
            .create(&Default::default(), &ingressroutetcp)
            .await?;
    }
    let pvc_api: Api<PersistentVolumeClaim> =
        Api::namespaced(ingressroutetcp_api.into_client(), challenge_ns);
    for pvc in pvcs {
        pvc_api.create(&Default::default(), &pvc).await?;
    }

    Ok(())
}
