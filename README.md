# richclip - Command line clipboard utility for multiple platforms

`richclip` is created for [richclip.nvim](https://github.com/beeender/richclip.nvim)
which intends to copy rich text format of source code from neovim with
highlights to the system clipboard. But it can also be used as a standalone
command line utility as an alternative to [xclip](https://github.com/astrand/xclip),
[xsel](https://github.com/astrand/xclip), [wl-clipboard](https://github.com/bugaevc/wl-clipboard)
or other command line clipboard tools.

- Supports Multiple environments. Xorg and Wayland are supported by the current
  version. MacOS and Windows are planned.
- Recognizes the environment automatically, and choose the right clipboard to
  use.
- Supports multiple content with different mime-types simultaneously. A typical
  use case is to have the plain source code with mime-type `text/plain` and
  highlighted HTML format code with mime-type `text/html` copied, so the client
  can choose the preferred content type to paste.

## Installing

### Arch Linux

Install `richclip` from the [AUR](https://aur.archlinux.org/packages/richclip).

### Other Linux Distributions

Download the static linked binary from the [release page](https://github.com/beeender/richclip/releases).

### MacOS & Windows

Not supported yet

## Usage

### Paste

```
❯ richclip paste --help
Paste the data from clipboard to the output
Usage: richclip paste [OPTIONS]
Options:
  -l, --list-types        List the offered mime-types of the current clipboard only without the contents
  -t, --type <mime-type>  Specify the preferred mime-type to be pasted
  -p, --primary           Use the 'primary' clipboard
  -h, --help              Print help
```

### Copy

```
❯ richclip copy --help
Receive and copy data to the clipboard
Usage: richclip copy [OPTIONS]
Options:
  -p, --primary     Use the 'primary' clipboard
      --foreground  Run in foreground
  -h, --help        Print help
```

The data to be copied to the clipboard needs to follow a simple protocol which
is described as below. A simpler transfer mode will be supported in the future
for copying single type content like other clipboard utilities.

| Item             | Bytes    | Content             |
|------------------| :------- | :------------------ |
| Magic            | 4        | 0x20 0x09 0x02 0x14 |
| Protocol Version | 1        | 0x00                |
| Section Type     | 1        | 'M'                 |
| Section Length   | 4        | 0x00 0x00 0x00 0x0a |
| Section Data     | 4        | "text/plain"        |
| Section Type     | 1        | 'M'                 |
| Section Length   | 4        | 0x00 0x00 0x00 0x04 |
| Section Data     | 4        | "TEXT"              |
| Section Type     | 1        | 'C'                 |
| Section Length   | 4        | 0x00 0x00 0x00 0x09 |
| Section Data     | 4        | "SOME Data"         |
| Section Type     | 1        | 'M'                 |
| Section Length   | 4        | 0x00 0x00 0x00 0x09 |
| Section Data     | 4        | "text/html"         |
| Section Type     | 1        | 'C'                 |
| Section Length   | 4        | 0x00 0x00 0x00 0x09 |
| Section Data     | 4        | "HTML code"         |

- Every section starts with the section type, `M` (mime-type) or `C` (content).
- Before `C` section, there must be one or more `M` section to indicate the data type.
- Section length will be parsed as big-endian uint32 number.
