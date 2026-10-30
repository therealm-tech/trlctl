use url::Url;

/// Whether `url` is a Git repository URL in a form Argo CD accepts
/// (SPEC.md §8.2): `http(s)://`, `ssh://`, `git://`, or scp-like `user@host:path`.
#[must_use]
pub fn is_git_url(url: &str) -> bool {
    if let Ok(parsed) = Url::parse(url) {
        return matches!(parsed.scheme(), "https" | "http" | "ssh" | "git")
            && parsed.host_str().is_some_and(|h| !h.is_empty())
            && parsed.path().len() > 1;
    }
    scp_like(url).is_some()
}

/// Whether `url` designates a Helm chart repository (`http(s)://`) or an OCI
/// registry written without a scheme (`host/path`), as SPEC.md §8.2 requires
/// when `chart` is set.
#[must_use]
pub fn is_chart_repository_url(url: &str) -> bool {
    if url.contains("://") {
        return Url::parse(url).is_ok_and(|parsed| {
            matches!(parsed.scheme(), "https" | "http")
                && parsed.host_str().is_some_and(|h| !h.is_empty())
        });
    }
    is_schemeless_oci(url)
}

/// The `https://` form of a Git remote, without credentials or SSH port.
/// `None` when `remote` is not a URL a host can be read from (a local path).
#[must_use]
pub fn to_https(remote: &str) -> Option<String> {
    let remote = remote.trim();
    if let Ok(mut parsed) = Url::parse(remote) {
        return match parsed.scheme() {
            "https" | "http" => {
                parsed.host_str()?;
                parsed.set_username("").ok()?;
                parsed.set_password(None).ok()?;
                Some(parsed.to_string())
            }
            "ssh" | "git" | "git+ssh" | "ssh+git" => {
                let host = parsed.host_str()?;
                Some(format!("https://{host}{}", parsed.path()))
            }
            _ => None,
        };
    }
    let (host, path) = scp_like(remote)?;
    Some(format!("https://{host}/{}", path.trim_start_matches('/')))
}

/// Whether two repository URLs designate the same repository, whatever their
/// transport (`ssh` or `https`), credentials or trailing `.git`.
#[must_use]
pub fn same_repository(a: &str, b: &str) -> bool {
    let normalise = |url: &str| {
        to_https(url).map(|https| {
            https
                .trim_end_matches('/')
                .trim_end_matches(".git")
                .to_lowercase()
        })
    };
    match (normalise(a), normalise(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn scp_like(url: &str) -> Option<(&str, &str)> {
    if url.contains("://") {
        return None;
    }
    let (user_host, path) = url.split_once(':')?;
    let (user, host) = user_host.split_once('@')?;
    let valid = !user.is_empty()
        && !host.is_empty()
        && !host.contains('/')
        && !path.is_empty()
        && !path.starts_with("//");
    valid.then_some((host, path))
}

fn is_schemeless_oci(url: &str) -> bool {
    let Some((host, path)) = url.split_once('/') else {
        return false;
    };
    let (hostname, port) = match host.split_once(':') {
        Some((hostname, port)) => (hostname, Some(port)),
        None => (host, None),
    };
    let hostname_ok = !hostname.is_empty()
        && hostname
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
        && (hostname.contains('.') || hostname == "localhost" || port.is_some());
    let port_ok = port.is_none_or(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    let path_ok = path
        .split('/')
        .all(|segment| !segment.is_empty() && !segment.contains(char::is_whitespace));
    hostname_ok && port_ok && path_ok
}

#[cfg(test)]
mod tests {
    use super::{is_chart_repository_url, is_git_url, same_repository, to_https};

    #[test]
    fn converts_scp_like_remotes() {
        assert_eq!(
            to_https("git@github.com:therealm-tech/layout-spec.git").as_deref(),
            Some("https://github.com/therealm-tech/layout-spec.git")
        );
        assert_eq!(
            to_https("git@gitlab.example.com:/group/sub/repo").as_deref(),
            Some("https://gitlab.example.com/group/sub/repo")
        );
    }

    #[test]
    fn converts_ssh_and_git_scheme_remotes() {
        assert_eq!(
            to_https("ssh://git@github.com/org/repo.git").as_deref(),
            Some("https://github.com/org/repo.git")
        );
        assert_eq!(
            to_https("ssh://git@git.example.com:2222/org/repo.git").as_deref(),
            Some("https://git.example.com/org/repo.git")
        );
        assert_eq!(
            to_https("git://git.example.com/org/repo.git").as_deref(),
            Some("https://git.example.com/org/repo.git")
        );
        assert_eq!(
            to_https("git+ssh://git@github.com/org/repo").as_deref(),
            Some("https://github.com/org/repo")
        );
    }

    #[test]
    fn keeps_https_and_strips_credentials() {
        assert_eq!(
            to_https("https://github.com/org/repo.git").as_deref(),
            Some("https://github.com/org/repo.git")
        );
        assert_eq!(
            to_https("https://user:secret@github.com/org/repo.git").as_deref(),
            Some("https://github.com/org/repo.git")
        );
        assert_eq!(
            to_https("https://oauth2@gitlab.com/org/repo").as_deref(),
            Some("https://gitlab.com/org/repo")
        );
        assert_eq!(
            to_https("  http://git.local/org/repo.git\n").as_deref(),
            Some("http://git.local/org/repo.git")
        );
    }

    #[test]
    fn refuses_local_paths_and_unknown_schemes() {
        assert_eq!(to_https("/srv/git/repo.git"), None);
        assert_eq!(to_https("../repo"), None);
        assert_eq!(to_https("file:///srv/git/repo.git"), None);
        assert_eq!(to_https("github.com:org/repo"), None);
    }

    #[test]
    fn recognises_git_urls() {
        for url in [
            "https://github.com/org/repo.git",
            "http://git.local/org/repo",
            "ssh://git@github.com/org/repo.git",
            "git@github.com:org/repo.git",
            "git://git.example.com/repo",
        ] {
            assert!(is_git_url(url), "{url}");
        }
        for url in [
            "ghcr.io/org/charts",
            "oci://ghcr.io/org/charts",
            "https://github.com",
            "repo",
            "",
        ] {
            assert!(!is_git_url(url), "{url}");
        }
    }

    #[test]
    fn recognises_chart_repositories() {
        for url in [
            "ghcr.io/argoproj/argo-helm",
            "registry.example.com/charts",
            "localhost:5000/charts",
            "https://charts.example.com",
            "https://charts.example.com/stable",
        ] {
            assert!(is_chart_repository_url(url), "{url}");
        }
        for url in [
            "oci://ghcr.io/argoproj/argo-helm",
            "git@github.com:org/repo.git",
            "ssh://git@github.com/org/repo.git",
            "argo-helm",
            "ghcr.io",
            "ghcr.io//charts",
        ] {
            assert!(!is_chart_repository_url(url), "{url}");
        }
    }

    #[test]
    fn compares_repositories_across_transports() {
        assert!(same_repository(
            "git@github.com:Org/Repo.git",
            "https://github.com/org/repo"
        ));
        assert!(same_repository(
            "https://token@github.com/org/repo.git/",
            "ssh://git@github.com/org/repo"
        ));
        assert!(!same_repository(
            "https://github.com/org/repo",
            "https://github.com/org/other"
        ));
        assert!(!same_repository("/local/repo", "/local/repo"));
    }
}
