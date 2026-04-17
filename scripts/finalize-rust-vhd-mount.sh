#!/usr/bin/env bash
set -euo pipefail

if [[ ${EUID} -ne 0 ]]; then
  cat >&2 <<'EOF'
Run as root. Example:
  /mnt/c/Windows/System32/wsl.exe -d Ubuntu -u root -- \
    /home/rose/work/codex/fork/scripts/finalize-rust-vhd-mount.sh /dev/sdX
EOF
  exit 1
fi

if [[ $# -ne 1 ]]; then
  echo "usage: $0 /dev/sdX" >&2
  echo "" >&2
  echo "Available block devices:" >&2
  lsblk -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINT >&2
  exit 1
fi

dev="$1"
if [[ ! -b "$dev" ]]; then
  echo "not a block device: $dev" >&2
  exit 1
fi

mkdir -p /mnt/rust-build

fstype="$(lsblk -no FSTYPE "$dev" | tr -d '[:space:]')"
if [[ -z "$fstype" ]]; then
  echo "No filesystem found on $dev; creating ext4 ..."
  mkfs.ext4 -F "$dev"
  fstype="ext4"
fi

if [[ "$fstype" != "ext4" ]]; then
  echo "Expected ext4 on $dev, found: $fstype" >&2
  exit 1
fi

if ! mountpoint -q /mnt/rust-build; then
  mount "$dev" /mnt/rust-build
fi

uuid="$(blkid -s UUID -o value "$dev" | tr -d '[:space:]')"
if [[ -z "$uuid" ]]; then
  echo "Could not read UUID for $dev" >&2
  exit 1
fi

fstab_line="UUID=$uuid /mnt/rust-build ext4 defaults,nofail 0 2"
if grep -q ' /mnt/rust-build ' /etc/fstab; then
  sed -i '\# /mnt/rust-build #d' /etc/fstab
fi
echo "$fstab_line" >> /etc/fstab

echo "Mounted: $dev -> /mnt/rust-build"
echo "UUID: $uuid"
df -h /mnt/rust-build
