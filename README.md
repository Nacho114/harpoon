# harpoon

A [Zellij](https://zellij.dev) plugin for quickly searching
and switching between tabs.

Copy of the original [harpoon](https://github.com/ThePrimeagen/harpoon) for nvim.

![usage](https://github.com/Nacho114/harpoon/raw/main/img/usage.gif)

## Usage

- `a` to add pane to list
- `A` to add all current panes to list
- `Up` and `Down` or `j` and `k` to cycle through pane list
- `d` to remove pane from list
- `Enter` or `l` to switch to the selected pane
- `/` to enter search mode
- `Esc` or `Ctrl + c` to exit

### Fuzzy Search

Press `/` to enter search mode. Type characters to fuzzy-match against pane tab names and titles (characters must appear in order but not necessarily contiguous). Matched characters are highlighted in the list.

In search mode:
- `Up` / `Down` to navigate filtered results
- `Enter` to jump to the selected pane
- `Backspace` to delete characters (exits search mode when empty)
- `Esc` to cancel search and return to normal mode

## Why?

In a sentence: Quickly access your most used panes.

- Manually manage list of favorite panes
- Easily add/remove from this list
- Use list to quickly go to pane
- Panes are automatically removed from your list when they are closed
- When tabs or panes change name, these changes propagate to your harpoon list

## Installation

**Requires Zellij `0.38.0` or newer.**

_Note_: you will need to have `wasm32-wasip1` added to rust as a target to build the plugin. This can be done with `rustup target add wasm32-wasip1`.

```bash
git clone git@github.com:Nacho114/harpoon.git
cd harpoon
cargo build --release
mkdir -p ~/.config/zellij/plugins/
cp target/wasm32-wasip1/release/harpoon.wasm ~/.config/zellij/plugins/
cp target/wasm32-wasip1/release/harpoon-worker.wasm ~/.config/zellij/plugins/
```

## Keybinding

Add the following to your [zellij config](https://zellij.dev/documentation/configuration.html)
somewhere inside the [keybinds](https://zellij.dev/documentation/keybindings.html) section:

```kdl
shared_except "locked" {
    bind "Ctrl y" {
        LaunchOrFocusPlugin "file:~/.config/zellij/plugins/harpoon.wasm" {
            floating true; move_to_focused_tab true;
        }
    }
}
```

> You likely already have a `shared_except "locked"` section in your configs. Feel free to add `bind` there.

## Recent Sort Mode

Enable `recent_sort` to sort the pane list by last accessed time (like JetBrains' Recent Files). The current focused pane is hidden from the list, so the first item is always your previous pane.

```kdl
shared_except "locked" {
    bind "Ctrl y" {
        LaunchOrFocusPlugin "file:~/.config/zellij/plugins/harpoon.wasm" {
            floating true; move_to_focused_tab true;
            recent_sort "true"
        }
    }
}
```

This mode requires the **harpoon-worker** background plugin to track pane focus changes while harpoon is closed. Add it to your layout as a hidden pane:

```kdl
// In your layout file (e.g. ~/.config/zellij/layouts/default.kdl)
pane size=1 borderless=true {
    plugin location="file:~/.config/zellij/plugins/harpoon-worker.wasm"
}
```

Or load it in your zellij config startup:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/harpoon-worker.wasm"
}
```

The worker continuously monitors which pane has focus and writes timestamps to `~/.local/share/zellij-harpoon/{session}-timestamps.json`. When harpoon opens, it reads this file to sort panes by recency.

## Contributing

If you find any issues or want to suggest ideas please [open an issue](https://github.com/Nacho114/harpoon/issues/new).

### Development

Make sure you have [rust](https://rustup.rs/) installed then run:

```sh
zellij action new-tab --layout ./plugin-dev-workspace.kdl
```
