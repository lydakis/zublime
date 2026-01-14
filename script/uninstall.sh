#!/usr/bin/env sh
set -eu

# Uninstalls Zublime that was installed using the install.sh script

check_remaining_installations() {
    platform="$(uname -s)"
    if [ "$platform" = "Darwin" ]; then
        # Check for any Zublime variants in /Applications
        remaining=$(ls -d /Applications/Zublime*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    else
        # Check for any Zublime variants in ~/.local
        remaining=$(ls -d "$HOME/.local/zublime"*.app 2>/dev/null | wc -l)
        [ "$remaining" -eq 0 ]
    fi
}

prompt_remove_preferences() {
    printf "Do you want to keep your Zublime preferences? [Y/n] "
    read -r response
    case "$response" in
        [nN]|[nN][oO])
            rm -rf "$HOME/.config/zublime"
            echo "Preferences removed."
            ;;
        *)
            echo "Preferences kept."
            ;;
    esac
}

main() {
    platform="$(uname -s)"
    channel="${ZED_CHANNEL:-stable}"

    if [ "$platform" = "Darwin" ]; then
        platform="macos"
    elif [ "$platform" = "Linux" ]; then
        platform="linux"
    else
        echo "Unsupported platform $platform"
        exit 1
    fi

    "$platform"

    echo "Zublime has been uninstalled"
}

linux() {
    suffix=""
    if [ "$channel" != "stable" ]; then
        suffix="-$channel"
    fi

    appid=""
    db_suffix="stable"
    case "$channel" in
      stable)
        appid="ooo.engineered.Zublime"
        db_suffix="stable"
        ;;
      nightly)
        appid="ooo.engineered.Zublime-Nightly"
        db_suffix="nightly"
        ;;
      preview)
        appid="ooo.engineered.Zublime-Preview"
        db_suffix="preview"
        ;;
      dev)
        appid="ooo.engineered.Zublime-Dev"
        db_suffix="dev"
        ;;
      *)
        echo "Unknown release channel: ${channel}. Using stable app ID."
        appid="ooo.engineered.Zublime"
        db_suffix="stable"
        ;;
    esac

    # Remove the app directory
    rm -rf "$HOME/.local/zublime$suffix.app"

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/zublime"

    # Remove the .desktop file
    rm -f "$HOME/.local/share/applications/${appid}.desktop"

    # Remove the database directory for this channel
    rm -rf "$HOME/.local/share/zublime/db/0-$db_suffix"

    # Remove socket file
    rm -f "$HOME/.local/share/zublime/zed-$db_suffix.sock"

    # Remove the entire Zublime directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/.local/share/zublime"
        prompt_remove_preferences
    fi

    rm -rf $HOME/.zed_server
}

macos() {
    app="Zublime.app"
    db_suffix="stable"
    app_id="ooo.engineered.Zublime"
    case "$channel" in
      nightly)
        app="Zublime Nightly.app"
        db_suffix="nightly"
        app_id="ooo.engineered.Zublime-Nightly"
        ;;
      preview)
        app="Zublime Preview.app"
        db_suffix="preview"
        app_id="ooo.engineered.Zublime-Preview"
        ;;
      dev)
        app="Zublime Dev.app"
        db_suffix="dev"
        app_id="ooo.engineered.Zublime-Dev"
        ;;
    esac

    # Remove the app bundle
    if [ -d "/Applications/$app" ]; then
        rm -rf "/Applications/$app"
    fi

    # Remove the binary symlink
    rm -f "$HOME/.local/bin/zublime"

    # Remove the database directory for this channel
    rm -rf "$HOME/Library/Application Support/Zublime/db/0-$db_suffix"

    # Remove app-specific files and directories
    rm -rf "$HOME/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.ApplicationRecentDocuments/$app_id.sfl"*
    rm -rf "$HOME/Library/Caches/$app_id"
    rm -rf "$HOME/Library/HTTPStorages/$app_id"
    rm -rf "$HOME/Library/Preferences/$app_id.plist"
    rm -rf "$HOME/Library/Saved Application State/$app_id.savedState"

    # Remove the entire Zublime directory if no installations remain
    if check_remaining_installations; then
        rm -rf "$HOME/Library/Application Support/Zublime"
        rm -rf "$HOME/Library/Logs/Zublime"

        prompt_remove_preferences
    fi

    rm -rf $HOME/.zed_server
}

main "$@"
