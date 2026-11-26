fn main() {
    let build_number = std::env::var("SOURCE_DATE_EPOCH").unwrap_or_else(|_| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("build clock")
            .as_secs()
            .to_string()
    });
    println!("cargo:rustc-env=POWERWATCH_BUILD_NUMBER={build_number}");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let root = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let directory = root.join("../../../assets/data/address-catalog");
    let index = directory.join("index.json");
    println!("cargo:rerun-if-changed={}", index.display());
    let data = std::fs::read(&index)
        .expect(
            "Підготуйте обласні пакети: scripts/Install-Catalog.py; див. README.md у корені репозиторію",
        );
    let expected = directory
        .parent()
        .unwrap()
        .join("address-catalog-manifest.json");
    println!("cargo:rerun-if-changed={}", expected.display());
    assert_eq!(
        data,
        std::fs::read(expected).expect("catalog version manifest"),
        "Пакети не відповідають затвердженому знімку каталогу"
    );
    let packages: Vec<serde_json::Value> =
        serde_json::from_slice(&data).expect("validated catalog index");
    assert_eq!(packages.len(), 27, "required 27 oblast packages");
    let mut code = String::from("const PACKAGES: &[(&str, &[u8], &str)] = &[\n");
    for package in packages {
        let id = package["oblastId"].as_str().unwrap();
        assert!(
            id.starts_with("UA") && id.len() == 19 && id[2..].bytes().all(|b| b.is_ascii_digit())
        );
        let archive = directory
            .join(format!("{id}.sqlite3.xz"))
            .canonicalize()
            .unwrap();
        let manifest = directory
            .join(format!("{id}.sqlite3.json"))
            .canonicalize()
            .unwrap();
        code.push_str(&format!(
            "({id:?}, include_bytes!({archive:?}), include_str!({manifest:?})),\n"
        ));
    }
    code.push_str("];\n");
    std::fs::write(
        std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("catalog_packages.rs"),
        code,
    )
    .unwrap();
    tauri_build::build();
}
