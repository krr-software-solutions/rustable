#!/bin/sh
# Create the GitLab release for a version tag: upload the archives in dist/ to the project's
# generic package registry, then create the release with a link to each.
#
# Run by CI's `gitlab-release` job (needs curl + jq, and CI_JOB_TOKEN can upload packages and
# create releases). Each link also gets a version-less permalink:
#   <project>/-/releases/permalink/latest/downloads/rustable-sql-<target>.tar.gz
set -eu

tag="${CI_COMMIT_TAG:?CI_COMMIT_TAG is not set}"
version="${tag#v}"
packages="$CI_API_V4_URL/projects/$CI_PROJECT_ID/packages/generic/rustable-sql/$version"

links='[]'
for file in dist/*.tar.gz dist/*.tar.gz.sha256; do
  name="$(basename "$file")"
  curl --fail-with-body --silent --show-error \
    --header "JOB-TOKEN: $CI_JOB_TOKEN" \
    --upload-file "$file" "$packages/$name"
  echo
  # rustable-sql-1.2.3-x86_64-unknown-linux-musl.tar.gz → rustable-sql-x86_64-unknown-linux-musl.tar.gz
  stable="$(printf '%s' "$name" | sed "s/^rustable-sql-$version-/rustable-sql-/")"
  links="$(printf '%s' "$links" | jq --arg name "$name" --arg url "$packages/$name" --arg path "/$stable" \
    '. + [{name: $name, url: $url, direct_asset_path: $path, link_type: "package"}]')"
done

description="$(cat <<MD
## rustable $version

**Binary** (\`rustable\`): download the archive for your platform below, or use the image:

\`\`\`sh
docker pull $CI_REGISTRY_IMAGE:$version
\`\`\`

**Crates** ([crates.io](https://crates.io/crates/rustable-sql)):

\`\`\`sh
cargo install rustable-sql-cli --version $version   # the CLI
cargo add rustable-sql@$version                     # the library + #[derive(Table)]
\`\`\`
MD
)"

jq -n --arg tag "$tag" --arg name "rustable $version" --arg description "$description" --argjson links "$links" \
  '{tag_name: $tag, name: $name, description: $description, assets: {links: $links}}' |
  curl --fail-with-body --silent --show-error \
    --header "JOB-TOKEN: $CI_JOB_TOKEN" --header "Content-Type: application/json" \
    --data @- "$CI_API_V4_URL/projects/$CI_PROJECT_ID/releases"
echo
