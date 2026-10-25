`playr-icons.ttf` is [emoji-icon-font](http://jslegers.github.io/emoji-icon-font/) cut to the five glyphs the window draws, U+23ED, U+23EE, U+23F5, U+23F8 and U+23F9, under the MIT license in `emoji-icon-font-mit-license.txt`. Made from the copy in `epaint_default_fonts` 0.36.2:

```sh
uv run --with fonttools pyftsubset emoji-icon-font.ttf \
  --unicodes=U+23F5,U+23F8,U+23EE,U+23F9,U+23ED --output-file=playr-icons.ttf
```
