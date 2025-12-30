// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub fn main() {
    tonic_prost_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .emit_rerun_if_changed(true)
        .build_server(false)
        .build_client(true)
        .compile_protos(
            &[
                "../manager/protos/challenges.proto",
                "../manager/protos/repository.proto",
            ],
            &["../manager/protos"],
        )
        .unwrap();
}
