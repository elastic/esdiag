#[cfg(test)]
mod tests {
    use crate::processor::diagnostic::data_source::get_sources;
    use semver::Version;

    #[test]
    fn test_semver_parsing_and_matching() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        // Let's test a simple one, like aliases
        let alias = es_sources.get("cat_aliases").unwrap();

        let v_0_9 = Version::parse("0.9.0").unwrap();
        let v_5_0 = Version::parse("5.0.0").unwrap();
        let v_5_1_1 = Version::parse("5.1.1").unwrap();
        let v_6_0 = Version::parse("6.0.0").unwrap();

        assert_eq!(alias.get_url(&v_0_9).unwrap(), "/_cat/aliases?v");
        assert_eq!(alias.get_url(&v_5_0).unwrap(), "/_cat/aliases?v");
        assert_eq!(
            alias.get_url(&v_5_1_1).unwrap(),
            "/_cat/aliases?v&s=alias,index"
        );
        assert_eq!(
            alias.get_url(&v_6_0).unwrap(),
            "/_cat/aliases?v&s=alias,index"
        );
    }

    #[test]
    fn test_semver_snapshots() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        // snapshot should strip prerelease
        let ilm = es_sources.get("ilm_explain").unwrap();
        
        let v_8 = Version::parse("8.0.0-SNAPSHOT").unwrap();
        assert_eq!(ilm.get_url(&v_8).unwrap(), "/*/_ilm/explain?human&expand_wildcards=all");
    }

    #[test]
    fn test_file_path_generation() {
        let sources = get_sources();
        let es_sources = sources.get("elasticsearch").unwrap();

        let alias = es_sources.get("cat_aliases").unwrap();
        assert_eq!(alias.get_file_path("cat_aliases"), "cat/cat_aliases.txt");

        let tasks = es_sources.get("tasks").unwrap();
        assert_eq!(tasks.get_file_path("tasks"), "tasks.json"); // no subdir, default extension is json if missing from yaml
    }
}
