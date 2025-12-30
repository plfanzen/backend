// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub fn main() {
    tonic_prost_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .emit_rerun_if_changed(true)
        .build_server(true)
        .build_client(false)
        .compile_protos(
            &["./protos/challenges.proto", "./protos/repository.proto"],
            &["./protos"],
        )
        .unwrap();
}
