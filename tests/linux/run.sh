#!/usr/bin/env bash
# Wi-Live on Linux, end to end, in Docker (HID-Fi's tests/docker/vm.sh gives a Mac a
# Linux VM on an external drive):   HIDFI_VM_HOME=/Volumes/HIDFi-Linux tests/linux/run.sh
# Cargo's caches live in Docker volumes, so a rerun builds only what changed.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
if [ -n "${HIDFI_VM_HOME:-}" ] && [ -z "${DOCKER_HOST:-}" ]; then
  eval "$("$HOME/HID-Fi/tests/docker/vm.sh" env)"
fi
docker build -q -t wilive-linux "$here" >/dev/null
# The sources go in on stdin: the VM shares only some of the Mac's folders.
tar -C "$repo" --exclude=./target --exclude=./.git --exclude=./tests/.out -cf - . | \
  docker run --rm -i -v wilive-cargo:/root/.cargo/registry -v wilive-target:/work/target \
  wilive-linux sh -c 'mkdir -p /src && tar -C /src -xf - && sh /src/tests/linux/inside.sh'
