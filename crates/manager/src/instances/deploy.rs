use k8s_openapi::api::apps::v1::Deployment;
use kube::{Api, Client};

use crate::repo::challenges::manifest::ChallengeYml;

pub async fn deploy_challenge(
    kube_client: &Client,
    challenge_ns: &str,
    challenge: ChallengeYml,
    exposed_domain: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let (deployments, svcs, ingressroutes, ingressroutestcp) = challenge.services.into_iter().fold(
        (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        |(mut deployments, mut svcs, mut ingressroutes, mut ingressroutestcp), (svc_id, svc)| {
            deployments.push(svc.get_deployment(svc_id.clone()));
            if let Some(internal_svc) = svc.get_internal_svc(svc_id.clone()) {
                svcs.push(internal_svc);
            }
            if let Some(external_svc) = svc.get_external_svc(svc_id.clone()) {
                svcs.push(external_svc);
            }
            if let Some(ir) = svc.get_ingress_route(svc_id.clone(), challenge_ns, exposed_domain) {
                ingressroutes.push(ir);
            }
            if let Some(irtcp) = svc.get_ingress_route_tcp(svc_id.clone(), challenge_ns, exposed_domain) {
                ingressroutestcp.push(irtcp);
            }
            (deployments, svcs, ingressroutes, ingressroutestcp)
        },
    );

    let deployment_api: Api<Deployment> = Api::namespaced(kube_client.clone(), challenge_ns);
    for deployment in deployments {
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

    Ok(())
}
