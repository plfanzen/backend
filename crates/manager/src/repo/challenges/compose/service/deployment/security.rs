// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use compose_spec::service::IdOrName;

use crate::repo::challenges::compose::service::ComposeServiceError;

/// Builds pod security context from compose service configuration
pub fn build_pod_security_context(
    svc: &compose_spec::Service,
) -> Option<k8s_openapi::api::core::v1::PodSecurityContext> {
    let mut pod_sec_ctx = k8s_openapi::api::core::v1::PodSecurityContext::default();
    let mut has_context = false;

    // Supplemental groups from group_add
    if !svc.group_add.is_empty() {
        if let Ok(groups) = parse_supplemental_groups(&svc.group_add) {
            pod_sec_ctx.supplemental_groups = Some(groups);
            has_context = true;
        }
    }

    if has_context {
        Some(pod_sec_ctx)
    } else {
        None
    }
}

fn parse_supplemental_groups(
    group_add: &indexmap::IndexSet<IdOrName>,
) -> Result<Vec<i64>, ComposeServiceError> {
    let mut groups: Vec<i64> = Vec::new();
    for group in group_add {
        if let IdOrName::Id(gid) = group {
            groups.push(*gid as i64);
        } else if group.as_name().is_some_and(|n| n == "root") {
            groups.push(0);
        } else {
            return Err(ComposeServiceError::Other(
                "Group names are not supported in 'group_add' field".to_string(),
            ));
        }
    }
    Ok(groups)
}

/// Builds container security context from compose service configuration
pub fn build_container_security_context(
    svc: &compose_spec::Service,
) -> Result<Option<k8s_openapi::api::core::v1::SecurityContext>, ComposeServiceError> {
    let mut ctx = k8s_openapi::api::core::v1::SecurityContext::default();
    let mut has_context = false;

    if svc.privileged {
        ctx.privileged = Some(true);
        has_context = true;
    }

    if let Some(user) = &svc.user {
        // Parse user string (format: "uid[:gid]")
        if let IdOrName::Id(uid) = user.user {
            ctx.run_as_user = Some(uid as i64);
            has_context = true;
        } else if user.user.as_name().is_some_and(|n| n == "root") {
            ctx.run_as_user = Some(0);
            has_context = true;
        } else {
            return Err(ComposeServiceError::UserNameNotSupported);
        }
    }

    if svc.read_only {
        ctx.read_only_root_filesystem = Some(true);
        has_context = true;
    }

    if !svc.cap_add.is_empty() {
        let add_caps: Vec<String> = svc.cap_add.iter().map(|cap| cap.to_string()).collect();
        ctx.capabilities = Some(k8s_openapi::api::core::v1::Capabilities {
            add: Some(add_caps),
            ..Default::default()
        });
        has_context = true;
    }

    if !svc.cap_drop.is_empty() {
        let drop_caps: Vec<String> = svc.cap_drop.iter().map(|cap| cap.to_string()).collect();
        if ctx.capabilities.is_none() {
            ctx.capabilities = Some(k8s_openapi::api::core::v1::Capabilities {
                drop: Some(drop_caps),
                ..Default::default()
            });
        } else if let Some(capabilities) = &mut ctx.capabilities {
            capabilities.drop = Some(drop_caps);
        }
        has_context = true;
    }

    Ok(if has_context { Some(ctx) } else { None })
}
