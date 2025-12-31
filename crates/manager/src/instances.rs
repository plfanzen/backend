// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use k8s_openapi::api::core::v1::{Namespace, Pod};
use kube::{Api, Client, api::ListParams};
use rand::Rng;
use std::collections::HashMap;

pub mod deploy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceState {
    Creating,
    Running,
    Terminating,
}

pub async fn is_instance_running(
    kube_client: &Client,
    challenge_id: &str,
    instance_id: &str,
) -> bool {
    let api: Api<Pod> = Api::namespaced(
        kube_client.clone(),
        format!(
            "challenge-{}-instance-{}",
            challenge_id, instance_id
        )
        .as_str(),
    );
    // Check if all pods are running, if not (or there are none), return false
    let lp = ListParams::default();
    let pod_list = api.list(&lp).await.expect("Failed to list pods");
    if pod_list.items.is_empty() {
        return false;
    }
    for pod in pod_list {
        if let Some(status) = pod.status {
            if let Some(phase) = status.phase {
                if phase != "Running" && phase != "Succeeded" {
                    return false;
                }
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

pub async fn get_instances(
    kube_client: &Client,
    challenge_id: &str,
    actor_id: &str,
) -> HashMap<String, InstanceState> {
    let api: Api<Namespace> = Api::all(kube_client.clone());
    let lp = ListParams::default()
        .labels(format!("challenge_id={},actor_id={}", challenge_id, actor_id).as_str());
    let ns_list = api.list(&lp).await.expect("Failed to list namespaces");
    let mut instances = HashMap::new();
    for ns in ns_list {
        if let Some(name) = ns.metadata.name {
            let state = if ns.metadata.deletion_timestamp.is_some()
                || ns
                    .status
                    .as_ref()
                    .is_some_and(|s| s.phase.as_deref() == Some("Terminating"))
            {
                InstanceState::Terminating
            } else if is_instance_running(
                kube_client,
                challenge_id,
                &name
                    .strip_prefix(
                        format!("challenge-{}-instance-", challenge_id).as_str(),
                    )
                    .unwrap_or(&name)
                    .to_string(),
            )
            .await
            {
                InstanceState::Running
            } else {
                InstanceState::Creating
            };
            let name = name
                .strip_prefix(
                    format!("challenge-{}-actor-{}-instance-", challenge_id, actor_id).as_str(),
                )
                .unwrap_or(&name)
                .to_string();
            instances.insert(name, state);
        }
    }
    instances
}

pub async fn prepare_instance(
    kube_client: &Client,
    challenge_id: &str,
    actor_id: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let api: Api<Namespace> = Api::all(kube_client.clone());
    // Ensure we have at most 5 instances
    let instances = get_instances(kube_client, challenge_id, actor_id).await;
    if instances.len() >= 5 {
        return Err("Too many pending instances".into());
    }
    // If we have one or more running instances, return an error
    if instances
        .values()
        .any(|state| matches!(state, InstanceState::Running | InstanceState::Creating))
    {
        return Err("An instance is already running/creating".into());
    }
    // This will never cause an infinite loop because we check the number of existing instances above
    loop {
        let instance_suffix: String = (0..12)
            .map(|_| format!("{:x}", rand::rng().random_range(0..16)))
            .collect();
        let instance_name = format!(
            "challenge-{}-instance-{}",
            challenge_id, instance_suffix
        );
        if api.get_opt(&instance_name).await?.is_some() {
            continue;
        }
        let ns = Namespace {
            metadata: kube::api::ObjectMeta {
                name: Some(instance_name.clone()),
                labels: Some(
                    [
                        ("challenge_id".to_string(), challenge_id.to_string()),
                        ("actor_id".to_string(), actor_id.to_string()),
                    ]
                    .iter()
                    .cloned()
                    .collect(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        api.create(&kube::api::PostParams::default(), &ns).await?;
        return Ok(instance_name);
    }
}

pub async fn delete_instance(
    kube_client: &Client,
    challenge_id: &str,
    actor_id: &str,
    instance_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let api: Api<Namespace> = Api::all(kube_client.clone());
    let instance_ns = format!(
        "challenge-{}-instance-{}",
        challenge_id, instance_id
    );
    let ns = api.get(&instance_ns).await?;
    if ns.metadata.labels.as_ref().and_then(|l| l.get("actor_id")) != Some(&actor_id.to_string()) {
        return Err("Instance does not belong to actor".into());
    }
    api.delete(&instance_ns, &kube::api::DeleteParams::default())
        .await?;
    Ok(())
}
