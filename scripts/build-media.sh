#!/usr/bin/env bash
# Ubuntu 24.04: gcc-mingw-w64-x86-64-posix g++-mingw-w64-x86-64-posix nasm meson ninja-build pkg-config
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/artifacts/media-build"
mkdir -p "$out/sources" "$out/ffmpeg/bin"
cd "$out"
ffmpeg=5a54fcf75e0245111075b1c31593ba1919c25306
dav1d=42b2b24fb8819f1ed3643aa9cf2a62f03868e3aa
curl -fL --retry 3 "https://codeload.github.com/FFmpeg/FFmpeg/tar.gz/$ffmpeg" -o sources/ffmpeg.tar.gz
curl -fL --retry 3 "https://codeload.github.com/videolan/dav1d/tar.gz/$dav1d" -o sources/dav1d.tar.gz
echo '928f8014c5840c21542a38cc51a3add6064f64572be10d558dbff48d3b50aae7  sources/ffmpeg.tar.gz' | sha256sum -c -
echo 'e26d41e2f496c1598f418726b871ce252ce9f18f8dbe3ad199349a42ed2cb02f  sources/dav1d.tar.gz' | sha256sum -c -
tar xf sources/ffmpeg.tar.gz
tar xf sources/dav1d.tar.gz
cat > cross.ini <<'CROSS'
[binaries]
c = 'x86_64-w64-mingw32-gcc-posix'
cpp = 'x86_64-w64-mingw32-g++-posix'
ar = 'x86_64-w64-mingw32-ar'
strip = 'x86_64-w64-mingw32-strip'
windres = 'x86_64-w64-mingw32-windres'
pkgconfig = 'pkg-config'
[host_machine]
system = 'windows'
cpu_family = 'x86_64'
cpu = 'x86_64'
endian = 'little'
CROSS
meson setup dav1d-build "dav1d-$dav1d" --cross-file cross.ini --prefix "$out/prefix" \
    --libdir lib --buildtype release --default-library static -Denable_tools=false -Denable_tests=false
ninja -C dav1d-build install
export PKG_CONFIG_LIBDIR="$out/prefix/lib/pkgconfig"
cd "FFmpeg-$ffmpeg"
# A tarball inside our checkout must not inherit the launcher's Git version.
printf 'git-%s\n' "$ffmpeg" > FF_VERSION
./configure --target-os=mingw32 --arch=x86_64 --enable-cross-compile \
    --cross-prefix=x86_64-w64-mingw32- --cc=x86_64-w64-mingw32-gcc-posix \
    --cxx=x86_64-w64-mingw32-g++-posix --pkg-config=pkg-config --pkg-config-flags=--static \
    --disable-autodetect --disable-network --disable-doc --disable-debug --disable-ffplay \
    --enable-libdav1d --extra-ldflags=-static
make -j"$(nproc)" ffmpeg.exe ffprobe.exe
x86_64-w64-mingw32-strip ffmpeg.exe ffprobe.exe
cp ffmpeg.exe ffprobe.exe "$out/ffmpeg/bin/"
cp COPYING.LGPLv2.1 "$out/ffmpeg/LICENSE.txt"
cp "$out/dav1d-$dav1d/COPYING" "$out/ffmpeg/LICENSE-dav1d.txt"
cp ffbuild/config.log config.h FF_VERSION "$out/sources/"
cp "$root/scripts/build-media.sh" "$out/sources/"
cp "$out/cross.ini" "$out/sources/"
dpkg-query -W > "$out/sources/build-packages.txt"
printf 'FFmpeg %s\ndav1d %s\nBuilt by scripts/build-media.sh; local file decoding, no network protocols.\n' "$ffmpeg" "$dav1d" > "$out/ffmpeg/SOURCES.txt"
cp "$out/ffmpeg/SOURCES.txt" "$out/sources/"
tar -czf "$out/media-sources.tar.gz" -C "$out" sources
