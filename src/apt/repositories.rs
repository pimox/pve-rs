#[perlmod::package(name = "PVE::RS::APT::Repositories", lib = "pve_rs")]
mod export {
    use std::convert::TryInto;

    use anyhow::{bail, Error};
    use serde::{Deserialize, Serialize};

    use proxmox_apt::repositories::{
        APTRepositoryFile, APTRepositoryFileError, APTRepositoryHandle, APTRepositoryInfo,
        APTStandardRepository,
    };

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    /// Result for the repositories() function
    pub struct RepositoriesResult {
        /// Successfully parsed files.
        pub files: Vec<APTRepositoryFile>,

        /// Errors for files that could not be parsed or read.
        pub errors: Vec<APTRepositoryFileError>,

        /// Common digest for successfully parsed files.
        pub digest: String,

        /// Additional information/warnings about repositories.
        pub infos: Vec<APTRepositoryInfo>,

        /// Standard repositories and their configuration status.
        pub standard_repos: Vec<APTStandardRepository>,
    }

    #[derive(Deserialize, Serialize)]
    #[serde(rename_all = "kebab-case")]
    /// For changing an existing repository.
    pub struct ChangeProperties {
        /// Whether the repository should be enabled or not.
        pub enabled: Option<bool>,
    }

    /// Get information about configured and standard repositories.
    #[export]
    pub fn repositories() -> Result<RepositoriesResult, Error> {
        let (files, errors, digest) = proxmox_apt::repositories::repositories()?;
        let digest = proxmox::tools::digest_to_hex(&digest);

        let suite = proxmox_apt::repositories::get_current_release_codename()?;

        let infos = proxmox_apt::repositories::check_repositories(&files)?;
        let standard_repos =
            proxmox_apt::repositories::standard_repositories(&files, "pve", &suite);

        Ok(RepositoriesResult {
            files,
            errors,
            digest,
            infos,
            standard_repos,
        })
    }

    /// Add the repository identified by the `handle`.
    /// If the repository is already configured, it will be set to enabled.
    ///
    /// The `digest` parameter asserts that the configuration has not been modified.
    #[export]
    pub fn add_repository(handle: &str, digest: Option<&str>) -> Result<(), Error> {
        let (mut files, errors, current_digest) = proxmox_apt::repositories::repositories()?;

        let handle: APTRepositoryHandle = handle.try_into()?;
        let suite = proxmox_apt::repositories::get_current_release_codename()?;

        if let Some(digest) = digest {
            let expected_digest = proxmox::tools::hex_to_digest(digest)?;
            if expected_digest != current_digest {
                bail!("detected modified configuration - file changed by other user? Try again.");
            }
        }

        // check if it's already configured first
        for file in files.iter_mut() {
            for repo in file.repositories.iter_mut() {
                if repo.is_referenced_repository(handle, "pve", &suite) {
                    if repo.enabled {
                        return Ok(());
                    }

                    repo.set_enabled(true);
                    file.write()?;

                    return Ok(());
                }
            }
        }

        let (repo, path) =
            proxmox_apt::repositories::get_standard_repository(handle, "pve", &suite);

        if let Some(error) = errors.iter().find(|error| error.path == path) {
            bail!(
                "unable to parse existing file {} - {}",
                error.path,
                error.error,
            );
        }

        if let Some(file) = files.iter_mut().find(|file| file.path == path) {
            file.repositories.push(repo);

            file.write()?;
        } else {
            let mut file = match APTRepositoryFile::new(&path)? {
                Some(file) => file,
                None => bail!("invalid path - {}", path),
            };

            file.repositories.push(repo);

            file.write()?;
        }

        Ok(())
    }

    /// Change the properties of the specified repository.
    ///
    /// The `digest` parameter asserts that the configuration has not been modified.
    #[export]
    pub fn change_repository(
        path: &str,
        index: usize,
        options: ChangeProperties,
        digest: Option<&str>,
    ) -> Result<(), Error> {
        let (mut files, errors, current_digest) = proxmox_apt::repositories::repositories()?;

        if let Some(digest) = digest {
            let expected_digest = proxmox::tools::hex_to_digest(digest)?;
            if expected_digest != current_digest {
                bail!("detected modified configuration - file changed by other user? Try again.");
            }
        }

        if let Some(error) = errors.iter().find(|error| error.path == path) {
            bail!("unable to parse file {} - {}", error.path, error.error);
        }

        if let Some(file) = files.iter_mut().find(|file| file.path == path) {
            if let Some(repo) = file.repositories.get_mut(index) {
                if let Some(enabled) = options.enabled {
                    repo.set_enabled(enabled);
                }

                file.write()?;
            } else {
                bail!("invalid index - {}", index);
            }
        } else {
            bail!("invalid path - {}", path);
        }

        Ok(())
    }
}
