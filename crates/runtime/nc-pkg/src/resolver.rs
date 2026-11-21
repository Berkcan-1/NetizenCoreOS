//! Bounded Package Dependency & Conflict Resolver (Feature 42).
//!
//! Resolves package installation graphs with explicit bounds, detecting:
//! - Missing dependencies
//! - Cyclic dependencies
//! - Version constraint mismatches
//! - Architecture incompatibilities

use alloc::vec::Vec;

pub const MAX_RESOLVE_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    MissingDependency,
    CyclicDependency,
    VersionConflict,
    ArchMismatch,
    DepthExceeded,
}

#[derive(Clone, Copy)]
pub struct DepSpec<'a> {
    pub app_id: &'a str,
    pub min_version: u32,
    pub max_version: u32, // 0 = unbounded upper
}

#[derive(Clone, Copy)]
pub struct RepoPackage<'a> {
    pub app_id: &'a str,
    pub version: u32,
    pub arch: &'a str,
    pub deps: &'a [DepSpec<'a>],
}

fn visit<'a>(
    target: &'a str,
    arch: &str,
    repo: &'a [RepoPackage<'a>],
    visiting: &mut Vec<&'a str>,
    order: &mut Vec<&'a str>,
    depth: usize,
) -> Result<(), ResolveError> {
    if depth > MAX_RESOLVE_DEPTH {
        return Err(ResolveError::DepthExceeded);
    }
    if order.contains(&target) {
        return Ok(());
    }
    if visiting.contains(&target) {
        return Err(ResolveError::CyclicDependency);
    }
    let pkg = repo
        .iter()
        .find(|p| p.app_id == target)
        .ok_or(ResolveError::MissingDependency)?;
    if pkg.arch != arch && pkg.arch != "any" {
        return Err(ResolveError::ArchMismatch);
    }
    visiting.push(target);
    for dep in pkg.deps {
        let dep_pkg = repo
            .iter()
            .find(|p| p.app_id == dep.app_id)
            .ok_or(ResolveError::MissingDependency)?;
        if dep_pkg.version < dep.min_version
            || (dep.max_version != 0 && dep_pkg.version > dep.max_version)
        {
            return Err(ResolveError::VersionConflict);
        }
        visit(dep.app_id, arch, repo, visiting, order, depth + 1)?;
    }
    visiting.pop();
    order.push(target);
    Ok(())
}

/// Return a topological install order (dependencies first) for `root_app_id`.
pub fn resolve_install_order<'a>(
    root_app_id: &'a str,
    target_arch: &str,
    repo: &'a [RepoPackage<'a>],
) -> Result<Vec<&'a str>, ResolveError> {
    let mut visiting = Vec::new();
    let mut order = Vec::new();
    visit(root_app_id, target_arch, repo, &mut visiting, &mut order, 0)?;
    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_orders_deps_and_detects_cycles_conflicts_and_arch() {
        let repo = [
            RepoPackage {
                app_id: "nc.ui.kit",
                version: 2,
                arch: "aarch64",
                deps: &[],
            },
            RepoPackage {
                app_id: "nc.editor",
                version: 1,
                arch: "aarch64",
                deps: &[DepSpec {
                    app_id: "nc.ui.kit",
                    min_version: 2,
                    max_version: 5,
                }],
            },
            RepoPackage {
                app_id: "nc.old.app",
                version: 1,
                arch: "aarch64",
                deps: &[DepSpec {
                    app_id: "nc.ui.kit",
                    min_version: 3,
                    max_version: 0,
                }],
            },
            RepoPackage {
                app_id: "nc.x86.only",
                version: 1,
                arch: "x86_64",
                deps: &[],
            },
        ];

        assert_eq!(
            resolve_install_order("nc.editor", "aarch64", &repo),
            Ok(alloc::vec!["nc.ui.kit", "nc.editor"])
        );
        assert_eq!(
            resolve_install_order("nc.old.app", "aarch64", &repo),
            Err(ResolveError::VersionConflict)
        );
        assert_eq!(
            resolve_install_order("nc.x86.only", "aarch64", &repo),
            Err(ResolveError::ArchMismatch)
        );

        let cyclic = [
            RepoPackage {
                app_id: "a",
                version: 1,
                arch: "aarch64",
                deps: &[DepSpec { app_id: "b", min_version: 1, max_version: 0 }],
            },
            RepoPackage {
                app_id: "b",
                version: 1,
                arch: "aarch64",
                deps: &[DepSpec { app_id: "a", min_version: 1, max_version: 0 }],
            },
        ];
        assert_eq!(
            resolve_install_order("a", "aarch64", &cyclic),
            Err(ResolveError::CyclicDependency)
        );
    }
}
