import io

# ═══ 1. PayrollView: scroll the form into view when New Employee opens ═══
p = r"C:\Users\riman\Desktop\Projects\TitaouPosCRM\src\routes\payroll\PayrollView.svelte"
src = io.open(p, encoding="utf-8").read()
old = """    fieldResult = '';
    fieldError = '';
    isAddOpen = true;
  }"""
new = """    fieldResult = '';
    fieldError = '';
    isAddOpen = true;
    // The form sits above the card grid — bring it into view (owner: the
    // button 'didn't work' when the field-team cards pushed it out of sight).
    setTimeout(() => {
      document.getElementById('newEmployeeForm')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 60);
  }"""
assert old in src
src = src.replace(old, new, 1)
old_fn = """  {#if isAddOpen}
    <div class="bg-pos-card border border-pos-border rounded-2xl p-5 shadow-md space-y-4 animate-in zoom-in-95 duration-150">"""
new_fn = """  {#if isAddOpen}
    <div id="newEmployeeForm" class="bg-pos-card border border-pos-border rounded-2xl p-5 shadow-md space-y-4 animate-in zoom-in-95 duration-150 scroll-mt-4">"""
assert old_fn in src
src = src.replace(old_fn, new_fn, 1)
io.open(p, "w", encoding="utf-8", newline="\n").write(src)
print("payroll scroll ok")

# ═══ 2. ProductEditModal: name fields larger when Arabic hidden ═══
p2 = r"C:\Users\riman\Desktop\Projects\TitaouPosCRM\src\lib\components\ProductEditModal.svelte"
src2 = io.open(p2, encoding="utf-8").read()
old_fr = """              <div class="grid grid-cols-2 gap-3">
                <div class="relative">
                  <label class="block text-xs font-bold text-pos-muted mb-1">Product Name (Français) *</label>"""
new_fr = """              <div class="grid grid-cols-2 gap-3">
                <div class="relative {hideNameAr ? 'col-span-2' : ''}">
                  <label class="block text-xs font-bold text-pos-muted mb-1">Product Name (Français) *</label>"""
assert old_fr in src2
src2 = src2.replace(old_fr, new_fr, 1)
io.open(p2, "w", encoding="utf-8", newline="\n").write(src2)
print("modal enlarge ok")

# ═══ 3. TrucksView: auto-refresh every 30s ═══
p3 = r"C:\Users\riman\Desktop\Projects\TitaouPosCRM\src\routes\trucks\TrucksView.svelte"
src3 = io.open(p3, encoding="utf-8").read()
old_mount = "  onMount(load);"
new_mount = """  onMount(() => {
    load();
    // Auto-refresh: trips start/sell/close from OTHER terminals (seller app
    // / LAN) — the page must catch up by itself (owner request).
    const iv = setInterval(() => { if (!busy && !loading) load(); }, 30000);
    return () => clearInterval(iv);
  });"""
assert old_mount in src3
src3 = src3.replace(old_mount, new_mount, 1)
io.open(p3, "w", encoding="utf-8", newline="\n").write(src3)
print("trucks autorefresh ok")

# ═══ 4. Versions: 1.0.0 official ═══
p4 = r"C:\Users\riman\Desktop\Projects\TitaouPosCRM\src-tauri\tauri.conf.json"
src4 = io.open(p4, encoding="utf-8").read()
src4 = src4.replace('"version": "0.10.26"', '"version": "1.0.0"')
io.open(p4, "w", encoding="utf-8", newline="\n").write(src4)

p5 = r"C:\Users\riman\Desktop\Projects\TitaouPosCRM\package.json"
src5 = io.open(p5, encoding="utf-8").read()
src5 = src5.replace('"version": "0.10.26"', '"version": "1.0.0"')
io.open(p5, "w", encoding="utf-8", newline="\n").write(src5)
print("versions 1.0.0 ok")

# ═══ 5. Android print dialog: keep Close visible (cap + scroll content) ═══
p6 = r"C:\Users\riman\Desktop\Projects\TitaouCRM\app\lib\core\utils\print\bluetooth_print_dialog.dart"
src6 = io.open(p6, encoding="utf-8").read()
old_dlg = """      content: SizedBox(
        width: 320,
        child: _busy"""
new_dlg = """      content: ConstrainedBox(
        constraints: const BoxConstraints(maxHeight: 420),
        child: SizedBox(
        width: 320,
        child: SingleChildScrollView(
        child: _busy"""
assert old_dlg in src6
src6 = src6.replace(old_dlg, new_dlg, 1)
# close the added wrappers: the content Column ends before `      ),\n      actions: [`
old_close = """              ),
        ],
      ),
      ),
      actions: ["""
if old_close not in src6:
    # generic: find "      ],\n      ),\n      actions: [" and close wrappers
    old_close2 = """        ],
      ),
      actions: ["""
    new_close2 = """        ],
      ),
      ),
      ),
      actions: ["""
    assert old_close2 in src6, "closing wrapper anchor"
    src6 = src6.replace(old_close2, new_close2, 1)
io.open(p6, "w", encoding="utf-8", newline="\n").write(src6)
print("print dialog ok")

# ═══ 6. ESC/POS Arabic bidi: reverse Arabic runs so CP1256 thermal printers
#        print names in readable order (they render LTR glyph-by-glyph). ═══
AR = "'\\u0600-\\u06FF'"
p7 = r"C:\Users\riman\Desktop\Projects\TitaouCRM\app\lib\core\utils\print\escpos_receipt.dart"
src7 = io.open(p7, encoding="utf-8").read()
old_enc = """  /// Encodes with CP1256 (Arabic-capable); unmappable chars become '?'.
  List<int> _encode(String s) {
    const codec = Windows1256Codec();
    try {
      return codec.encode(s);
    } catch (_) {
      return s
          .replaceAllMapped(RegExp(r'[^\\x00-\\xFF]'), (_) => '?')
          .codeUnits;
    }
  }"""
new_enc = """  /// Encodes with CP1256 (Arabic-capable); unmappable chars become '?'.
  /// Thermal heads render CP1256 text GLYPH-BY-GLYPH left-to-right — Arabic
  /// would come out mirrored (and without shaping). Fix: reverse each Arabic
  /// run before encoding so the paper reads correctly.
  List<int> _encode(String s) {
    const codec = Windows1256Codec();
    final fixed = s.replaceAllMapped(RegExp(r'[\\u0600-\\u06FF]+'), (m) {
      final run = m[0]!;
      return String.fromCharCodes(run.codeUnits.reversed);
    });
    try {
      return codec.encode(fixed);
    } catch (_) {
      return fixed
          .replaceAllMapped(RegExp(r'[^\\x00-\\xFF]'), (_) => '?')
          .codeUnits;
    }
  }"""
assert old_enc in src7
src7 = src7.replace(old_enc, new_enc, 1)
io.open(p7, "w", encoding="utf-8", newline="\n").write(src7)
print("receipt bidi ok")

# same for the recap builder
p8 = r"C:\Users\riman\Desktop\Projects\TitaouCRM\app\lib\core\utils\print\escpos_trip_recap.dart"
src8 = io.open(p8, encoding="utf-8").read()
old_enc8 = """  List<int> _encode(String s) {
    const codec = Windows1256Codec();
    try {
      return codec.encode(s);
    } catch (_) {
      return s
          .replaceAllMapped(RegExp(r'[^\\x00-\\xFF]'), (_) => '?')
          .codeUnits;
    }
  }"""
new_enc8 = """  List<int> _encode(String s) {
    const codec = Windows1256Codec();
    // Arabic runs reversed — thermal CP1256 prints LTR glyph-by-glyph.
    final fixed = s.replaceAllMapped(RegExp(r'[\\u0600-\\u06FF]+'), (m) {
      final run = m[0]!;
      return String.fromCharCodes(run.codeUnits.reversed);
    });
    try {
      return codec.encode(fixed);
    } catch (_) {
      return fixed
          .replaceAllMapped(RegExp(r'[^\\x00-\\xFF]'), (_) => '?')
          .codeUnits;
    }
  }"""
assert old_enc8 in src8
src8 = src8.replace(old_enc8, new_enc8, 1)
io.open(p8, "w", encoding="utf-8", newline="\n").write(src8)
print("recap bidi ok")
