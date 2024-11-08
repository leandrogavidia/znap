use crate::template;
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Keypair;
use std::collections::HashMap;
use std::fs::{copy, create_dir, create_dir_all, read_dir, remove_dir_all};
use std::io::Write;
use std::process::{Child, Stdio};
use std::{
    fs::{read_to_string, File},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Status {
    pub active: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Collection {
    pub name: String,
    pub address: String,
    pub port: u16,
    pub protocol: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub collections: Option<Vec<Collection>>,
    pub identity: Option<String>,
    pub rpc_url: Option<String>,
}

pub fn get_cwd() -> PathBuf {
    std::env::current_dir().expect("Shoud be able to read cwd")
}

pub fn get_config() -> Config {
    let cwd = get_cwd();
    let znap_file_path = cwd.join("Znap.toml");
    let znap_file = read_to_string(znap_file_path)
        .expect("Should be able to read Znap.toml file. Make sure you are in a Znap workspace.");

    toml::from_str(&znap_file).expect("Znap.toml file should have the proper format")
}

fn get_identity(identity: &str) -> String {
    shellexpand::tilde(identity).into()
}

pub fn write_file(path: &Path, content: &str) {
    let mut file = File::create(path).expect("Should be able to open file");
    file.write_all(content.as_bytes())
        .unwrap_or_else(|_| panic!("Should be able to write file: {path:?}"));
}

pub fn get_envs(
    config: &Config,
    collection: &Collection,
    address: Option<&str>,
    port: Option<&u16>,
    protocol: Option<&str>,
) -> HashMap<&'static str, String> {
    let mut env_vars: HashMap<&str, String> = HashMap::new();

    if let Ok(path) = std::env::var("IDENTITY_KEYPAIR_PATH") {
        env_vars.insert("IDENTITY_KEYPAIR_PATH", get_identity(&path));
    } else if let Ok(v) = std::env::var("IDENTITY_KEYPAIR") {
        env_vars.insert("IDENTITY_KEYPAIR", v);
    } else if let Some(i) = config.identity.as_deref() {
        env_vars.insert("IDENTITY_KEYPAIR_PATH", get_identity(i));
    }

    if let Some(rpc_url) = &config.rpc_url {
        env_vars.insert("RPC_URL", rpc_url.to_string());
    }

    if let Some(address) = address.or(Some(&collection.address)) {
        env_vars.insert("COLLECTION_ADDRESS", address.to_owned());
    }

    if let Some(port) = port.or(Some(&collection.port)) {
        env_vars.insert("COLLECTION_PORT", port.to_string());
    }

    if let Some(protocol) = protocol.or(Some(&collection.protocol)) {
        env_vars.insert("COLLECTION_PROTOCOL", protocol.to_owned());
    }

    env_vars
}

pub fn start_server_blocking(
    config: &Config,
    collection: &Collection,
    address: Option<&str>,
    port: Option<&u16>,
    protocol: Option<&str>,
) {
    let start_server_process = start_server(config, collection, address, port, protocol);
    let exit = start_server_process
        .wait_with_output()
        .expect("Should be able to start server");

    if !exit.status.success() {
        std::process::exit(exit.status.code().unwrap_or(1));
    }
}

pub fn start_server(
    config: &Config,
    collection: &Collection,
    address: Option<&str>,
    port: Option<&u16>,
    protocol: Option<&str>,
) -> Child {
    std::process::Command::new("cargo")
        .envs(get_envs(config, collection, address, port, protocol))
        .arg("run")
        .arg("--manifest-path")
        .arg(get_cwd().join(format!(".znap/collections/{}/Cargo.toml", collection.name)))
        .arg("--bin")
        .arg("serve")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| anyhow::format_err!("{}", e.to_string()))
        .expect("Should be able to start server")
}

pub fn run_test_suite() {
    let output = std::process::Command::new("npm")
        .arg("run")
        .arg("test")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Should wait until the tests are over");

    if !output.status.success() {
        panic!("Test failed: {}", String::from_utf8_lossy(&output.stdout));
    }
}

pub fn wait_for_server(address: &str, port: &u16, protocol: &str) {
    let url = format!("{protocol}://{address}:{port}/status");

    loop {
        if let Ok(response) = reqwest::blocking::get(&url) {
            if let Ok(status) = response.json::<Status>() {
                if status.active {
                    break;
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
}

pub fn deploy_to_shuttle(project: &str, collection: &Collection) {
    std::process::Command::new("cargo")
        .arg("shuttle")
        .arg("deploy")
        .arg("--allow-dirty")
        .arg("--name")
        .arg(project)
        .arg("--working-directory")
        .arg(get_cwd().join(format!(".znap/collections/{}", collection.name)))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .expect("Should wait until the deploy is over");
}

pub fn copy_recursively(source: impl AsRef<Path>, destination: impl AsRef<Path>) {
    create_dir_all(&destination).unwrap();
    for entry in read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let filetype = entry.file_type().unwrap();
        if filetype.is_dir() {
            copy_recursively(entry.path(), destination.as_ref().join(entry.file_name()));
        } else {
            copy(entry.path(), destination.as_ref().join(entry.file_name())).unwrap();
        }
    }
}

pub fn get_identity_keypair(config: &Config, collection: &Collection) -> Keypair {
    let envs = get_envs(config, collection, None, None, None);

    match envs.get("IDENTITY_KEYPAIR") {
        Some(keypair) => Keypair::from_base58_string(keypair),
        _ => match envs.get("IDENTITY_KEYPAIR_PATH") {
            Some(path) => {
                let keypair_file = std::fs::read_to_string(path).unwrap();
                let keypair_bytes = keypair_file
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .split(',')
                    .map(|b| b.trim().parse::<u8>().unwrap())
                    .collect::<Vec<_>>();

                Keypair::from_bytes(&keypair_bytes).unwrap()
            }
            _ => panic!("Identity not valid."),
        },
    }
}

pub fn generate_collection_executable_files(config: &Config, collection: &Collection) {
    let cwd = get_cwd();
    let znap_path = cwd.join(".znap");

    if !znap_path.exists() {
        create_dir(&znap_path).expect("Could not create .znap folder");
    }

    let znap_toml_path = znap_path.join("Cargo.toml");

    write_file(
        &znap_toml_path,
        "[workspace]\nmembers = [\"collections/*\"]\nresolver = \"2\"\n\n[patch.crates-io]\ncurve25519-dalek = { git = \"https://github.com/dalek-cryptography/curve25519-dalek\", rev = \"8274d5cbb6fc3f38cdc742b4798173895cd2a290\" }",
    );

    let znap_collections_path = znap_path.join("collections");

    if !znap_collections_path.exists() {
        create_dir(&znap_collections_path).expect("Could not create .znap/collections folder");
    }

    let znap_collection_path = znap_collections_path.join(&collection.name);

    if znap_collection_path.exists() {
        remove_dir_all(&znap_collection_path)
            .unwrap_or_else(|_| panic!("Could not delete .znap/{} folder", &collection.name))
    }

    create_dir(&znap_collection_path)
        .unwrap_or_else(|_| panic!("Could not create .znap/{} folder", &collection.name));

    let identity_keypair = get_identity_keypair(config, collection);
    let rpc_url = config
        .rpc_url
        .as_ref()
        .unwrap_or_else(|| panic!("RPC url is not defined"));
    let secrets_content = format!(
        "IDENTITY_KEYPAIR=\"{}\"\nRPC_URL=\"{}\"",
        identity_keypair.to_base58_string(),
        rpc_url,
    );
    let secrets_path = znap_collection_path.join("Secrets.toml");

    write_file(&secrets_path, &secrets_content);

    let znap_collection_src_path = znap_collection_path.join("src");

    create_dir(&znap_collection_src_path)
        .unwrap_or_else(|_| panic!("Could not create .znap/{}/src folder", &collection.name));

    let znap_collection_src_bin_path = znap_collection_src_path.join("bin");

    create_dir(&znap_collection_src_bin_path)
        .unwrap_or_else(|_| panic!("Could not create .znap/{}/src/bin folder", &collection.name));

    let collection_path = cwd.join(format!("collections/{}", &collection.name));
    let collection_src_path = collection_path.join("src");

    copy_recursively(collection_src_path, znap_collection_src_path);

    // Generate the binaries
    let znap_collection_src_bin_serve_path = znap_collection_src_bin_path.join("serve.rs");
    write_file(
        &znap_collection_src_bin_serve_path,
        &template::collection_serve_binary::template(collection),
    );

    let znap_collection_src_bin_deploy_path = znap_collection_src_bin_path.join("deploy.rs");
    write_file(
        &znap_collection_src_bin_deploy_path,
        &template::collection_deploy_binary::template(&collection.name),
    );

    // Generate a toml with collection and extras for serve/deploy
    let znap_collection_toml_path = znap_collection_path.join("Cargo.toml");
    let collection_toml_path = collection_path.join("Cargo.toml");

    let mut collection_toml = read_to_string(collection_toml_path).expect("Cargo.toml not found");
    if let Ok(new_path) = std::env::var("ZNAP_LIB") {
        if let Some(line) = collection_toml
            .lines()
            .find(|l| l.trim().starts_with("znap = { path ="))
        {
            collection_toml =
                collection_toml.replace(line, &format!("znap = {{ path = \"{new_path}\" }}"));
        }
    }
    let znap_toml_extras = template::collection_toml::template(&collection.name);

    write_file(
        &znap_collection_toml_path,
        &format!("{collection_toml}\n{znap_toml_extras}"),
    );
}

pub fn build_for_release(name: &str) {
    std::process::Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(get_cwd().join(format!(".znap/collections/{name}/Cargo.toml")))
        .arg("--release")
        .arg("--bin")
        .arg("serve")
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(anyhow::Error::from)
        .expect("Should be able to build collection.")
        .wait_with_output()
        .expect("Should wait until the build is over");
}

pub fn init_git(dir: &PathBuf) {
    // Initialize a new git repository
    std::process::Command::new("git")
        .arg("init")
        .current_dir(dir)
        .status()
        .expect("Git project could not be initialized");

    // Rename the default branch to 'main'
    std::process::Command::new("git")
        .arg("branch")
        .arg("-M")
        .arg("main")
        .current_dir(dir)
        .status()
        .expect("Default branch could not be renamed to main");

    // Add all files to the staging area
    std::process::Command::new("git")
        .arg("add")
        .arg(".")
        .current_dir(dir)
        .status()
        .expect("Initial files could not be added to the staging area");

    // Commit the changes
    std::process::Command::new("git")
        .arg("commit")
        .arg("-m")
        .arg("initial commit")
        .current_dir(dir)
        .status()
        .expect("Initial commit could not be performed");
}
