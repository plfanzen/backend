// SPDX-FileCopyrightText: 2026 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::repo::challenges::compose::service::ComposeServiceError;

macro_rules! ensure_option_none {
    ($field:expr) => {
        if $field.is_some() {
            return Err(ComposeServiceError::PropertyNotSupported(
                stringify!($field).to_string(),
            ));
        }
    };
}

macro_rules! ensure_map_empty {
    ($field:expr) => {
        if !$field.is_empty() {
            return Err(ComposeServiceError::PropertyNotSupported(
                stringify!($field).to_string(),
            ));
        }
    };
}

macro_rules! ensure_false {
    ($field:expr) => {
        if $field {
            return Err(ComposeServiceError::PropertyNotSupported(
                stringify!($field).to_string(),
            ));
        }
    };
}

pub(super) use {ensure_false, ensure_map_empty, ensure_option_none};

pub fn ensure_only_supported(svc: &compose_spec::Service) -> Result<(), ComposeServiceError> {
    ensure_option_none!(svc.build);
    ensure_map_empty!(svc.storage_opt);
    ensure_map_empty!(svc.sysctls);
    ensure_map_empty!(svc.ulimits);
    ensure_option_none!(svc.mem_swappiness);
    ensure_option_none!(svc.memswap_limit);
    ensure_option_none!(svc.pid);
    ensure_option_none!(svc.pids_limit);
    ensure_option_none!(svc.network_config);
    ensure_option_none!(svc.mac_address);
    ensure_false!(svc.oom_kill_disable);
    ensure_option_none!(svc.oom_score_adj);
    ensure_option_none!(svc.platform);
    ensure_map_empty!(svc.security_opt);
    ensure_map_empty!(svc.profiles);
    Ok(())
}
