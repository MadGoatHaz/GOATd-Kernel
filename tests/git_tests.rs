use goatd_kernel::kernel::git::GitManager;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_git_manager_clone_and_operations() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let source_path = temp_dir.path().join("source");
    let clone_path = temp_dir.path().join("clone");

    // 1. Create a local source repository
    let repo = git2::Repository::init(&source_path).expect("Failed to init source repo");
    fs::write(source_path.join("README.md"), "# Test Repo").expect("Failed to write README");

    {
        let mut index = repo.index().expect("Failed to get index");
        index
            .add_path(std::path::Path::new("README.md"))
            .expect("Failed to add file");
        index.write().expect("Failed to write index");
        let tree_id = index.write_tree().expect("Failed to write tree");
        let tree = repo.find_tree(tree_id).expect("Failed to find tree");
        let sig = git2::Signature::now("Test User", "test@example.com")
            .expect("Failed to create signature");
        repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
            .expect("Failed to commit");
    }

    println!("Cloning from {:?} to {:?}", source_path, clone_path);

    // 2. Test Clone
    let url = source_path.to_str().unwrap();
    let manager = GitManager::clone(url, &clone_path).expect("Failed to clone repository");

    assert!(clone_path.exists());
    assert!(clone_path.join(".git").exists());
    assert!(clone_path.join("README.md").exists());

    // 3. Test list_branches
    let branches = manager.list_branches().expect("Failed to list branches");
    println!("Branches: {:?}", branches);
    assert!(!branches.is_empty());

    // 4. Test get_head_commit
    let head = manager
        .get_head_commit()
        .expect("Failed to get HEAD commit");
    println!("HEAD commit: {}", head);
    assert_eq!(head.len(), 40);

    // 5. Test repo_path
    assert_eq!(manager.repo_path(), clone_path);
}

#[test]
fn test_git_manager_invalid_url() {
    let url = "https://github.com/nonexistent/nonexistent_repo_12345";
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let clone_path = temp_dir.path().join("invalid_repo");

    let result = GitManager::clone(url, &clone_path);
    assert!(result.is_err());

    if let Err(e) = result {
        println!("Expected error: {}", e);
    }
}
