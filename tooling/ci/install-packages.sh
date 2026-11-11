#!/usr/bin/env bash
set -euo pipefail

# The packages our isolated tests need, from Ubuntu's archive as two mirrors serve it that share
# neither a domain nor a network: the runner's own, then kernel.org's when that one fails. The
# hosted runner's resolver can lose the Ubuntu archive domains, and kernel.org's address can be
# out of a runner's reach. Ubuntu's archive keyring verifies what either one hands over.
. /etc/os-release
sources=$(mktemp "$RUNNER_TEMP/dispatch-ubuntu-XXXXXX.sources")
chmod 644 "$sources"
apt_options=(-o "Dir::Etc::sourcelist=$sources" -o 'Dir::Etc::sourceparts=-' -o Acquire::Retries=2)

# PROOF ONLY, reverted before review: the runner's own mirror put out of reach, as kernel.org's
# address was, so this run installs through the fallback.
echo '192.0.2.1 azure.archive.ubuntu.com' | sudo tee -a /etc/hosts > /dev/null

for mirror in http://azure.archive.ubuntu.com/ubuntu https://mirrors.edge.kernel.org/ubuntu; do
  cat > "$sources" <<SOURCES
Types: deb
URIs: $mirror
Suites: $VERSION_CODENAME $VERSION_CODENAME-updates $VERSION_CODENAME-security
Components: main universe
Signed-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg
SOURCES
  if sudo apt-get "${apt_options[@]}" update --error-on=any -qq &&
    sudo apt-get "${apt_options[@]}" install -y "$@"; then
    exit
  fi
  echo "::warning::The packages could not be installed from $mirror."
done
exit 1
