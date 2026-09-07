#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 || $# -gt 5 ]]; then
  echo "usage: package-deb.sh <version> <architecture> <binary-directory> <output-directory> [edition]" >&2
  exit 2
fi

version="${1#v}"
version="${version#clear-v}"
architecture="$2"
binary_dir="$3"
output_dir="$4"
edition="${5:-upstream}"

case "$edition" in
  upstream)
    package_name="git-agent"
    display_name="Git Agent"
    asset_stem="GitAgent"
    install_dir="git-agent"
    main_executable="git-agent"
    ;;
  clear)
    package_name="git-agent-clear"
    display_name="Git Agent Clear"
    asset_stem="GitAgent-Clear"
    install_dir="git-agent-clear"
    main_executable="git-agent-clear"
    ;;
  *)
    echo "unknown edition: $edition (expected upstream or clear)" >&2
    exit 2
    ;;
esac

package_root="$output_dir/$package_name-deb"
install_root="$package_root/usr/lib/$install_dir"

rm -rf "$package_root"
mkdir -p \
  "$package_root/DEBIAN" \
  "$install_root" \
  "$package_root/usr/bin" \
  "$package_root/usr/share/applications" \
  "$package_root/usr/share/icons/hicolor/64x64/apps"

for executable in git-agent git-agent-merge git-agent-diff; do
  target_name="$executable"
  if [[ "$executable" == "git-agent" ]]; then
    target_name="$main_executable"
  fi
  install -m 755 "$binary_dir/$executable" "$install_root/$target_name"
  ln -s "../lib/$install_dir/$target_name" "$package_root/usr/bin/$target_name"
done

install -m 644 assets/icons/logo-ga.png \
  "$package_root/usr/share/icons/hicolor/64x64/apps/$package_name.png"

cat > "$package_root/usr/share/applications/$package_name.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$display_name
Comment=Desktop Git helper
Exec=/usr/lib/$install_dir/$main_executable
Icon=$package_name
Terminal=false
Categories=Development;RevisionControl;
StartupNotify=true
DESKTOP

installed_size=$(du -sk "$package_root/usr" | cut -f1)
cat > "$package_root/DEBIAN/control" <<CONTROL
Package: $package_name
Version: $version
Section: devel
Priority: optional
Architecture: $architecture
Installed-Size: $installed_size
Depends: libgtk-3-0, libx11-6, libxcb1, libxkbcommon0, libgl1
Maintainer: Git Agent <adoin@qq.com>
Homepage: https://github.com/awiggy/git-clear
Description: Desktop Git helper built with Rust and egui
 $display_name provides repository, history, diff, and merge workflows in a desktop application.
CONTROL

dpkg-deb --build --root-owner-group \
  "$package_root" \
  "$output_dir/${asset_stem}_${version}_${architecture}.deb"
rm -rf "$package_root"
