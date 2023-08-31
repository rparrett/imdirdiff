# imdirdiff

A tool for comparing two directories full of image files.

## Usage

```bash
$ find a -type f
a/same.png
a/different.png
a/a_only.png
a/c/recursive.png

$ find b -type f
b/same.png
b/different.png
b/b_only.png
b/c/recursive.png

$ cargo run --release a b
[-] a_only.png
[+] b_only.png
[≠] different.png
[≠] c/recursive.png

$ open imdirdiff-out/index.html
```
