#!/usr/bin/env bash
set -euo pipefail

# The hosted runner's resolver can lose the Ubuntu archive domains. Use a separate mirror,
# still verified by Ubuntu's archive keyring, for the packages our isolated tests need.
. /etc/os-release
sources=$(mktemp "$RUNNER_TEMP/dispatch-ubuntu-XXXXXX.sources")
cat > "$sources" <<SOURCES
Types: deb
URIs: https://mirrors.edge.kernel.org/ubuntu
Suites: $VERSION_CODENAME $VERSION_CODENAME-updates $VERSION_CODENAME-security
Components: main universe
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
SOURCES
chmod 644 "$sources"

apt_options=(-o "Dir::Etc::sourcelist=$sources" -o 'Dir::Etc::sourceparts=-' -o Acquire::Retries=2)
sudo apt-get "${apt_options[@]}" update --error-on=any -qq
sudo apt-get "${apt_options[@]}" install -y "$@"
