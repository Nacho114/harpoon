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

### Coming next:

In front of every pane in the list, there will be a character, this will allow you to navigate to any pane in two steps:
- open plugin
- type char corresponding to desired pane.

Makes pane navigation easier. 

## Installation

You'll need [rust](https://rustup.rs/) installed.

### With build script

- `chmod +x ./build.sh`
- `./build.sh`

### Manual

Harpoon depends on [cached](https://github.com/Nacho114/cached), another Zellij plugin.
Make sure you have it installed! 

> Note you can also add it to a [layout](https://zellij.dev/documentation/creating-a-layout.html#plugin) so that so that it directly runs. 

- `git clone git@github.com:Nacho114/harpoon.git`
- `cd harpoon`
- `cargo build --release`
- `mkdir -p ~/.config/zellij/plugins/`
- `mv target/wasm32-wasi/release/harpoon.wasm ~/.config/zellij/plugins/`

## Keybinding

Add the following to your [zellij config](https://zellij.dev/documentation/configuration.html)
somewhere inside the [keybinds](https://zellij.dev/documentation/keybindings.html) section:

```kdl
shared_except "locked" {
    bind "Ctrl y" {
        LaunchOrFocusPlugin "file:~/.config/zellij/plugins/harpoon.wasm" {
            floating true
        }
    }
}
```

> You likely already have a `shared_except "locked"` section in your configs. Feel free to add `bind` there.

## How to run

When zellij is running, you need to launch `cached` (which we just installed):

```shell
zellij action start-or-reload-plugin file:~/.config/zellij/plugins/cached.wasm
```

## Contributing

If you find any issues or want to suggest ideas please [open an issue](https://github.com/Nacho114/harpoon/issues/new).

### Development

Make sure you have [rust](https://rustup.rs/) installed then run:

```sh
zellij action new-tab --layout ./dev.kdl
```
