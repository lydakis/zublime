#!/usr/bin/env sh
set -eu

# Downloads the latest tarball from GitHub Releases and unpacks it
# into ~/.local/. If you'd prefer to do this manually, instructions are at
# https://github.com/lydakis/zublime.

main() {
    platform="$(uname -s)"
    arch="$(uname -m)"
    channel="${ZED_CHANNEL:-stable}"
    release_tag="${ZUBLIME_RELEASE_TAG:-${ZED_RELEASE_TAG:-}}"
    # Use TMPDIR if available (for environments with non-standard temp directories)
    if [ -n "${TMPDIR:-}" ] && [ -d "${TMPDIR}" ]; then
        temp="$(mktemp -d "$TMPDIR/zublime-XXXXXX")"
    else
        temp="$(mktemp -d "/tmp/zublime-XXXXXX")"
    fi

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    case "$platform-$arch" in
        macos-arm64* | linux-arm64* | linux-armhf | linux-aarch64)
            arch="aarch64"
            ;;
        macos-x86* | linux-x86* | linux-i686*)
            arch="x86_64"
            ;;
        *)
            echo "Unsupported platform or architecture"
            exit 1
            ;;
    esac

    if command -v curl >/dev/null 2>&1; then
        curl () {
            command curl -fL "$@"
        }
    elif command -v wget >/dev/null 2>&1; then
        curl () {
            wget -O- "$@"
        }
    else
        echo "Could not find 'curl' or 'wget' in your path"
        exit 1
    fi

    "$platform" "$@"

    if [ "$(command -v zublime)" = "$HOME/.local/bin/zublime" ]; then
        echo "Zublime has been installed. Run with 'zublime'"
    else
        echo "To run Zublime from your terminal, you must add ~/.local/bin to your PATH"
        echo "Run:"

        case "$SHELL" in
            *zsh)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.zshrc"
                echo "   source ~/.zshrc"
                ;;
            *fish)
                echo "   fish_add_path -U $HOME/.local/bin"
                ;;
            *)
                echo "   echo 'export PATH=\$HOME/.local/bin:\$PATH' >> ~/.bashrc"
                echo "   source ~/.bashrc"
                ;;
        esac

        echo "To run Zublime now, '~/.local/bin/zublime'"
    fi
}

linux() {
    download_base="https://github.com/lydakis/zublime/releases/latest/download"
    if [ -n "$release_tag" ]; then
        download_base="https://github.com/lydakis/zublime/releases/download/$release_tag"
    elif [ "$channel" != "stable" ]; then
        echo "Warning: non-stable channel '$channel' requested, but GitHub latest is stable. Set ZUBLIME_RELEASE_TAG to override."
    fi

    if [ -n "${ZED_BUNDLE_PATH:-}" ]; then
        cp "$ZED_BUNDLE_PATH" "$temp/zublime-linux-$arch.tar.gz"
    else
        echo "Downloading Zublime"
        curl "${download_base}/zublime-linux-$arch.tar.gz" > "$temp/zublime-linux-$arch.tar.gz"
    fi

    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    case "$channel" in
      stable)
        appid="ooo.engineered.Zublime"
        ;;
      nightly)
        appid="ooo.engineered.Zublime-Nightly"
        ;;
      preview)
        appid="ooo.engineered.Zublime-Preview"
        ;;
      dev)
        appid="ooo.engineered.Zublime-Dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="ooo.engineered.Zublime"
        ;;
    esac

    # Unpack
    rm -rf "$HOME/.local/zublime$suffix.app"
    mkdir -p "$HOME/.local/zublime$suffix.app"
    tar -xzf "$temp/zublime-linux-$arch.tar.gz" -C "$HOME/.local/"

    # Setup ~/.local directories
    mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"

    # Link the binary
    if [ -f "$HOME/.local/zublime$suffix.app/bin/zublime" ]; then
        ln -sf "$HOME/.local/zublime$suffix.app/bin/zublime" "$HOME/.local/bin/zublime"
    else
        # support for versions before 0.139.x.
        ln -sf "$HOME/.local/zublime$suffix.app/bin/cli" "$HOME/.local/bin/zublime"
    fi

    # Copy .desktop file
    desktop_file_path="$HOME/.local/share/applications/${appid}.desktop"
    cp "$HOME/.local/zublime$suffix.app/share/applications/zublime$suffix.desktop" "${desktop_file_path}"
    sed -i "s|Icon=zublime|Icon=$HOME/.local/zublime$suffix.app/share/icons/hicolor/512x512/apps/zublime.png|g" "${desktop_file_path}"
    sed -i "s|Exec=zublime|Exec=$HOME/.local/zublime$suffix.app/bin/zublime|g" "${desktop_file_path}"
}

macos() {
    download_base="https://github.com/lydakis/zublime/releases/latest/download"
    if [ -n "$release_tag" ]; then
        download_base="https://github.com/lydakis/zublime/releases/download/$release_tag"
    elif [ "$channel" != "stable" ]; then
        echo "Warning: non-stable channel '$channel' requested, but GitHub latest is stable. Set ZUBLIME_RELEASE_TAG to override."
    fi

    echo "Downloading Zublime"
    curl "${download_base}/Zublime-$arch.dmg" > "$temp/Zublime-$arch.dmg"
    hdiutil attach -quiet "$temp/Zublime-$arch.dmg" -mountpoint "$temp/mount"
    app="$(cd "$temp/mount/"; echo *.app)"
    echo "Installing $app"
    if [ -d "/Applications/$app" ]; then
        echo "Removing existing $app"
        rm -rf "/Applications/$app"
    fi
    ditto "$temp/mount/$app" "/Applications/$app"
    hdiutil detach -quiet "$temp/mount"

    mkdir -p "$HOME/.local/bin"
    # Link the binary
    ln -sf "/Applications/$app/Contents/MacOS/cli" "$HOME/.local/bin/zublime"
}

main "$@"
