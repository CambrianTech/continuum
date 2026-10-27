use super::*;

mod hosted_apis;
mod docker_runner;
mod llama_server_coders;
mod llama_server_frontier;
mod llama_server_catalog;
mod in_process;

pub fn models() -> Vec<Model> {
    let mut all = Vec::new();
    all.extend(hosted_apis::hosted_apis());
    all.extend(docker_runner::docker_runner());
    all.extend(llama_server_coders::llama_server_coders());
    all.extend(llama_server_frontier::llama_server_frontier());
    all.extend(llama_server_catalog::llama_server_catalog());
    all.extend(in_process::in_process());
    all
}
