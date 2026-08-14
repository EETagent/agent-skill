use agent_skill::remote::{RemoteClient, RemoteSkill};
use pretty_assertions::assert_eq;

#[test]
fn builds_an_installable_source_expression() {
    let skill = RemoteSkill {
        name: "alpha".to_owned(),
        slug: "owner/repo/alpha".to_owned(),
        source: "owner/repo#main@old-name".to_owned(),
        installs: 42,
    };

    assert_eq!(skill.repository_source(), "owner/repo#main@old-name");
    assert_eq!(skill.install_source(), "owner/repo#main@alpha");
}

#[test]
fn validates_configuration_without_network_access() {
    assert!(RemoteClient::new("file:///tmp/catalog").is_err());

    let client = RemoteClient::new("https://example.invalid").expect("HTTP client");
    assert!(client.search("", None, 20).is_err());
    assert!(client.search("rust", Some("-invalid"), 20).is_err());
    assert!(client.search("rust", None, 0).is_err());
}

#[test]
fn debug_output_redacts_api_credentials() {
    let client =
        RemoteClient::new("https://user:secret@example.invalid/catalog").expect("HTTP client");
    let debug = format!("{client:?}");

    assert!(!debug.contains("user"));
    assert!(!debug.contains("secret"));
}
