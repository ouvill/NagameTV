use arib_b24::{DecodedModule, Descriptor, Error, ModuleInfo};

fn module(data: &[u8], kind: Option<&[u8]>) -> DecodedModule {
    let mut descriptors = vec![Descriptor {
        tag: 2,
        data: b"entry.bml".to_vec(),
    }];
    if let Some(kind) = kind {
        descriptors.push(Descriptor {
            tag: 1,
            data: kind.to_vec(),
        });
    }
    DecodedModule {
        download_id: 7,
        info: ModuleInfo {
            id: 12,
            size: data.len() as u32,
            version: 2,
            descriptors,
        },
        data: data.to_vec(),
    }
}

#[test]
fn direct_module_is_one_named_resource() -> Result<(), Error> {
    let resources = module(b"<bml/>", Some(b"text/bml")).resources()?;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].module_name, b"entry.bml");
    assert_eq!(resources[0].name, b"entry.bml");
    assert_eq!(
        resources[0].media_type.as_deref(),
        Some(b"text/bml".as_slice())
    );
    assert_eq!(resources[0].data, b"<bml/>");
    Ok(())
}

#[test]
fn multipart_module_extracts_binary_resources_by_length() -> Result<(), Error> {
    let bytes = b"Content-Type: multipart/mixed; boundary=part\r\n\r\n\
--part\r\nContent-Type: text/bml\r\nContent-Location: main.bml\r\nContent-Length: 6\r\n\r\n<bml/>\r\n\
--part\r\nContent-Type: image/png\r\nContent-Location: images/icon.png\r\nContent-Length: 10\r\n\r\nA\r\n--partB\r\n\
--part--\r\n";
    let resources = module(bytes, None).resources()?;
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0].name, b"main.bml");
    assert_eq!(resources[0].data, b"<bml/>");
    assert_eq!(resources[1].name, b"images/icon.png");
    assert_eq!(
        resources[1].media_type.as_deref(),
        Some(b"image/png".as_slice())
    );
    assert_eq!(resources[1].data, b"A\r\n--partB");
    Ok(())
}

#[test]
fn multipart_module_keeps_nhk_resource_list_with_empty_location() -> Result<(), Error> {
    let bytes = b"Content-Type: multipart/mixed; boundary=part\r\n\r\n\
--part\r\nContent-Type: application/X-arib-resourceList\r\nContent-Location: \r\nContent-Length: 4\r\n\r\nlist\r\n\
--part\r\nContent-Type: text/X-arib-bml\r\nContent-Location: startup.bml\r\nContent-Length: 6\r\n\r\n<bml/>\r\n\
--part--\r\n";
    let resources = module(bytes, None).resources()?;
    assert_eq!(resources.len(), 2);
    assert!(resources[0].name.is_empty());
    assert_eq!(resources[0].data, b"list");
    assert_eq!(resources[1].name, b"startup.bml");
    assert_eq!(resources[1].data, b"<bml/>");
    Ok(())
}

#[test]
fn malformed_multipart_does_not_return_partial_resources() {
    let bytes = b"Content-Type: multipart/mixed; boundary=part\r\n\r\n\
--part\r\nContent-Type: text/bml\r\nContent-Location: main.bml\r\nContent-Length: 7\r\n\r\n<bml/>\r\n\
--part--\r\n";
    assert!(module(bytes, None).resources().is_err());
}

#[test]
fn entity_headers_allow_arbitrary_order_and_continuations() -> Result<(), Error> {
    let bytes = b"X_Broadcast: test\r\nContent-Type: multipart/mixed;\r\n\tboundary=part\r\n\r\n\
--part\r\nContent-Location: main.bml\r\nContent-Length: 6\r\nContent-Type: text/X-arib-bml;\r\n charset=\"UTF-8\"\r\n\r\n<bml/>\r\n--part--\r\n";
    for kind in [None, Some(b"multipart/mixed".as_slice())] {
        let resources = module(bytes, kind).resources()?;
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].name, b"main.bml");
        assert_eq!(resources[0].data, b"<bml/>");
        assert_eq!(
            resources[0].media_type.as_deref(),
            Some(b"text/X-arib-bml; charset=\"UTF-8\"".as_slice())
        );
    }
    // A declared direct mapping is authoritative even for header-like bytes.
    assert_eq!(
        module(bytes, Some(b"text/plain")).resources()?[0].data,
        bytes
    );
    assert!(
        module(
            b" continuation\r\nContent-Type: text/plain\r\n\r\nbody",
            None
        )
        .resources()
        .is_err()
    );
    Ok(())
}

#[test]
fn linked_module_is_not_misreported_as_a_complete_resource() {
    let mut fragment = module(b"first fragment", Some(b"image/png"));
    fragment.info.descriptors.push(Descriptor {
        tag: 0x04,
        data: vec![0, 0, 13],
    });
    assert!(matches!(
        fragment.resources(),
        Err(Error::LinkedModuleFragment)
    ));
}

#[test]
fn resource_save_uses_caller_path_and_does_not_overwrite() -> Result<(), Box<dyn std::error::Error>>
{
    let resource = module(b"image-bytes", Some(b"image/png"))
        .resources()?
        .remove(0);
    let path = std::env::temp_dir().join(format!(
        "arib-b24-resource-{}-{}.bin",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    resource.save_to(&path)?;
    assert_eq!(std::fs::read(&path)?, b"image-bytes");
    assert_eq!(
        resource.save_to(&path).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    std::fs::remove_file(path)?;
    Ok(())
}
