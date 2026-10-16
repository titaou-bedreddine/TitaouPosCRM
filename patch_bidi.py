import io

# ═══ Print dialog: cap height + scroll; Close always visible ═══
p6 = r"C:\Users\riman\Desktop\Projects\TitaouCRM\app\lib\core\utils\print\bluetooth_print_dialog.dart"
src6 = io.open(p6, encoding="utf-8").read()

old_open = """      content: SizedBox(
        width: 320,
        child: _busy"""
new_open = """      content: ConstrainedBox(
        constraints: const BoxConstraints(maxHeight: 400),
        child: SizedBox(
          width: 320,
          child: SingleChildScrollView(
            child: _busy"""
assert old_open in src6, "open anchor"
src6 = src6.replace(old_open, new_open, 1)

old_indent_busy = """            ? const Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CircularProgressIndicator(),
                  SizedBox(height: 12),
                  Text('Impression en cours... / Printing...'),
                ],
              )
            : Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: ["""
new_indent_busy = """                ? const Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      CircularProgressIndicator(),
                      SizedBox(height: 12),
                      Text('Impression en cours... / Printing...'),
                    ],
                  )
                : Column(
                    mainAxisSize: MainAxisSize.min,
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: ["""
assert old_indent_busy in src6, "busy anchor"
src6 = src6.replace(old_indent_busy, new_indent_busy, 1)

old_close = """                ],
              ),
      ),
      actions: ["""
new_close = """                ],
                ),
          ),
        ),
      ),
      actions: ["""
assert old_close in src6, "close anchor"
src6 = src6.replace(old_close, new_close, 1)
io.open(p6, "w", encoding="utf-8", newline="\n").write(src6)
print("dialog ok")

# ═══ ESC/POS Arabic bidi: reverse Arabic runs before CP1256 encode ═══
new_enc_template = """  List<int> _encode(String s) {{
    const codec = Windows1256Codec();
    // Thermal heads render CP1256 glyphs left-to-right - Arabic would come
    // out mirrored. Reverse each Arabic run so the paper reads correctly.
    final fixed = s.replaceAllMapped(RegExp(r'[\\u0600-\\u06FF]+'), (m) {{
      final run = m[0]!;
      return String.fromCharCodes(run.codeUnits.reversed);
    }});
    try {{
      return codec.encode(fixed);
    }} catch (_) {{
      return fixed
          .replaceAllMapped(RegExp(r'[^\\x00-\\xFF]'), (_) => '?')
          .codeUnits;
    }}
  }}"""

for p, name in [
    (r"C:\Users\riman\Desktop\Projects\TitaouCRM\app\lib\core\utils\print\escpos_receipt.dart", "receipt"),
    (r"C:\Users\riman\Desktop\Projects\TitaouCRM\app\lib\core\utils\print\escpos_trip_recap.dart", "recap"),
]:
    src = io.open(p, encoding="utf-8").read()
    old = """  List<int> _encode(String s) {
    const codec = Windows1256Codec();
    try {
      return codec.encode(s);
    } catch (_) {
      return s
          .replaceAllMapped(RegExp(r'[^\x00-\xFF]'), (_) => '?')
          .codeUnits;
    }
  }"""
    new = new_enc_template.format()
    assert old in src, name + " anchor"
    src = src.replace(old, new, 1)
    io.open(p, "w", encoding="utf-8", newline="\n").write(src)
    print(name, "bidi ok")
