mod ancestor {
    use crate::hex_to_id;
    use git_odb::linked;
    use git_odb::linked::Db;

    fn check_traversal(tip: &str, expected: &[&str]) -> crate::Result {
        let (_temp_dir, db) = db()?;
        let head = hex_to_id(tip);
        let oids: Result<Vec<_>, _> = git_odb::traverse::ancestors::Iter::new(&db, head).collect();
        let expected: Vec<_> = std::iter::once(head)
            .chain(expected.iter().map(|hex_id| hex_to_id(hex_id)))
            .collect();
        assert_eq!(oids?, expected);
        Ok(())
    }

    #[test]
    fn linear_history_no_branch() -> crate::Result {
        check_traversal(
            "9556057aee5abb06912922e9f26c46386a816822",
            &[
                "17d78c64cef6c33a10a604573fd2c429e477fd63",
                "9902e3c3e8f0c569b4ab295ddf473e6de763e1e7",
                "134385f6d781b7e97062102c6a483440bfda2a03",
            ],
        )
    }

    #[test]
    #[should_panic]
    fn simple_branch_with_merge() {
        check_traversal(
            "01ec18a3ebf2855708ad3c9d244306bc1fae3e9b",
            &[
                "efd9a841189668f1bab5b8ebade9cd0a1b139a37",
                "ce2e8ffaa9608a26f7b21afc1db89cadb54fd353",
                "9556057aee5abb06912922e9f26c46386a816822",
                "9152eeee2328073cf23dcf8e90c949170b711659",
                "17d78c64cef6c33a10a604573fd2c429e477fd63",
                "9902e3c3e8f0c569b4ab295ddf473e6de763e1e7",
                "134385f6d781b7e97062102c6a483440bfda2a03",
            ],
        )
        .unwrap()
    }

    fn db() -> Result<(tempdir::TempDir, Db), Box<dyn std::error::Error>> {
        let dir = crate::assure_fixture_repo_present("make_traversal_repo.sh")?;
        let db = linked::Db::at(dir.path().join(".git").join("objects"))?;
        Ok((dir, db))
    }
}
