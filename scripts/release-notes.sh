#!/bin/sh
# The release notes (markdown) for a version tag, shared by the GitLab and GitHub releases.
#
#   scripts/release-notes.sh v1.2.3      # uses DOCKERHUB_USER for the image name
set -eu

tag="${1:?usage: scripts/release-notes.sh <tag>}"
version="${tag#v}"
: "${DOCKERHUB_USER:?DOCKERHUB_USER is not set (the image on Docker Hub is docker.io/\$DOCKERHUB_USER/rustable-sql)}"

cat <<MD
## rustable $version

**Binary** (\`rustable\`): download the archive for your platform below, or use the image:

\`\`\`sh
docker pull $DOCKERHUB_USER/rustable-sql:$version
\`\`\`

**Crates** ([crates.io](https://crates.io/crates/rustable-sql)):

\`\`\`sh
cargo install rustable-sql-cli --version $version   # the CLI
cargo add rustable-sql@$version                     # the library + #[derive(Table)]
\`\`\`
MD
