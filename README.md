# harpoon

A [Zellij](https://zellij.dev) plugin for quickly searching
and switching between tabs.

Copy of the original [harpoon](https://github.com/ThePrimeagen/harpoon) for nvim.

![usage](https://github.com/Nacho114/harpoon/raw/main/img/usage.gif)

## Usage

- `a` to add pane to list
- `Up` and `Down` or `j` and `k` to cycle through pane list
- `d` to remove pane from list
- `Enter` or `l` to switch to the selected pane
- `Esc` or `Ctrl + c` to exit

## Why?

In a sentence: Quickly access your most used panes.

- Manually manage list of favorite panes
- Easily add/remove from this list
- Use list to quickly go to pane
- Panes are automatically removed from your list when they are closed
- When tabs or panes change name, these changes propagate to your harpoon list

## Installation

**Requires Zellij `0.38.0` or newer.**

*Note*: you will need to have `wasm32-wasi` added to rust as a target to build the plugin. This can be done with `rustup target add wasm32-wasi`.

```bash
git clone git@github.com:Nacho114/harpoon.git
cd harpoon
cargo build --release
mkdir -p ~/.config/zellij/plugins/
mv target/wasm32-wasi/release/harpoon.wasm ~/.config/zellij/plugins/
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

## Contributing

If you find any issues or want to suggest ideas please [open an issue](https://github.com/Nacho114/harpoon/issues/new).

### Development

Make sure you have [rust](https://rustup.rs/) installed then run:

```sh
zellij action new-tab --layout ./plugin-dev-workspace.kdl
```
