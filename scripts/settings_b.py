import io, re

path = 'src/routes/settings/SettingsView.svelte'
with io.open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()
src = ''.join(lines)

def cut(start_marker, end_marker, label, keep_end=True):
    """Remove text from start_marker (inclusive) to end_marker (exclusive)."""
    global src
    i = src.find(start_marker)
    j = src.find(end_marker, i + 1)
    assert i >= 0 and j > i, f'cut failed: {label} ({i},{j})'
    src = src[:i] + src[j:]
    print('cut:', label)

def rep(old, new, label=''):
    global src
    if old not in src:
        print('MISS:', label or old[:70])
        return
    src = src.replace(old, new)
    print('ok:', label or old[:50])

# ---- 1) barcode preview state + render fn + reactive (lines 248-277 area) ----
cut("  let settingsBarcodeSvgEl: SVGSVGElement;",
    "  // ----- Label presets (sticker + shelf saved configurations) -----",
    'barcode preview state/fn')

# The label-preset save/apply JSON system (whole block up to "// Factory Reset")
cut("  // ----- Label presets (sticker + shelf saved configurations) -----",
    "  // Factory Reset",
    'label_presets JSON system')

# ---- 2) onMount: drop loadLabelPresets ----
rep("""    loadLabelPresets().catch(() => {});
    loadShortcutBindings().catch(() => {});""",
    """    loadShortcutBindings().catch(() => {});""",
    'onMount loadLabelPresets removal')

# ---- 3) loadSettings: drop renderSettingsBarcode ----
rep("""      const h = await invoke<string>('get_hwid');
      if (h) hwid = h;
      await tick();
      renderSettingsBarcode();""",
    """      const h = await invoke<string>('get_hwid');
      if (h) hwid = h;""",
    'loadSettings barcode render removal')

# ---- 4) test print functions: replace the whole legacy block ----
i_start = src.find("  function testPrintReceipt() {")
i_end = src.find("  // ----- Built-in 40×20 mm thermal presets")
assert i_start > 0 and i_end > i_start, (i_start, i_end)
new_tests = """  // Unified receipt test print: the SAME builder the POS uses, printed via
  // the native silent pipeline with the configured paper width.
  let testPrintMsg = '';
  async function testPrintReceipt() {
    testPrintMsg = '';
    const qr = await entityQrDataUrl('SALE:TEST-0001', 240).catch(() => undefined);
    const built = buildUnifiedReceipt({
      saleNumber: 'TEST-0001',
      saleDate: new Date().toLocaleString('fr-FR'),
      cashierName: $currentUser?.display_name || 'Admin',
      customerName: 'Client Comptoir',
      paymentMethod: 'ESPÈCES',
      items: [
        { name: 'Eau Minérale 1.5L', quantity: 2, unitPrice: 120, totalPrice: 240 },
        { name: 'Lait UHT Entier 1L', quantity: 1, unitPrice: 150, totalPrice: 150 },
        { name: 'Café Moulu 250g', quantity: 1, unitPrice: 200, totalPrice: 200 },
      ],
      subtotal: 590,
      discount: 0,
      grandTotal: 590,
      amountPaid: 600,
      change: 10,
      settings: settings as Record<string, string>,
      qrDataUrl: qr,
    });
    const r = await printHtmlSilently(built.html, built.title, { widthMm: built.paperWidthMm });
    testPrintMsg = r.ok ? '✅ Test receipt sent to the printer (silent).' : '❌ ' + r.message;
  }

"""
src = src[:i_start] + new_tests + src[i_end:]
print('replaced legacy test-print block')

# ---- 5) builtin label test: no browser fallback ----
rep("""    } catch (e: any) {
      // Backend command unavailable (dev/old binary) → browser print.
      printHtmlDirectly(
        def.build(builtinLabelData),
        `Test ${def.name}`,
        { widthMm: def.widthMm, heightMm: def.heightMm }
      );
      builtinTestMsg = '⚠ Browser print fallback (exact-media backend not available)';
    }""",
    """    } catch (e: any) {
      builtinTestMsg = '❌ ' + (typeof e === 'string' ? e : e?.message || String(e));
    }""",
    'builtin label test fallback removal')

# ---- 6) drop the old testPrintProfessionalReceipt function (unified covers it) ----
i_p = src.find("  async function testPrintProfessionalReceipt() {")
i_p_end = src.find("</script>", i_p)
assert i_p > 0 and i_p_end > i_p, (i_p, i_p_end)
# keep the closing of script; find the actual end of the function: it ends with "  }\n\n</script>"
i_p_end = src.find("  }\n\n</script>", i_p)
assert i_p_end > i_p
src = src[:i_p] + src[i_p_end + len("  }\n\n"):]
print('dropped testPrintProfessionalReceipt')

with io.open(path, 'w', encoding='utf-8', newline='') as f:
    f.write(src)
print('chunk B done')
