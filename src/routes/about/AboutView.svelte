<script lang="ts">
  import {
    Info, Phone, MessageCircle, Globe, Video, Facebook, Instagram, Youtube as YoutubeIcon, Music2,
    ShieldCheck, Sparkles, Cpu, HardDrive, Database, Layers,
    CheckCircle2, ExternalLink
  } from 'lucide-svelte';

  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';

  const developerName = 'Titaou Bedreddine';
  const developerPhone = '0553444057';
  let appVersion = '...';

  onMount(async () => {
    try {
      appVersion = 'v' + (await invoke<string>('get_app_version')) + ' PRO (Offline Desktop Edition)';
    } catch {
      appVersion = 'PRO (Offline Desktop Edition)';
    }
  });

  function openExternal(url: string) {
    window.open(url, '_blank');
  }
</script>

<div class="p-6 overflow-y-auto h-full select-none bg-pos-bg flex flex-col space-y-6">
  <!-- Top Header Banner -->
  <div class="bg-gradient-to-r from-sky-600 to-blue-700 rounded-3xl p-6 text-white shadow-lg flex flex-col md:flex-row items-center justify-between gap-6 shrink-0">
    <div class="flex items-center gap-5 text-center md:text-start">
      <div class="w-20 h-20 rounded-2xl bg-white p-2 shadow-xl flex items-center justify-center shrink-0">
        <img src="/logo.png" alt="TitaouPosCRM Logo" class="w-full h-full object-contain" />
      </div>
      <div class="space-y-1">
        <div class="flex items-center gap-2 justify-center md:justify-start">
          <h1 class="text-2xl font-black tracking-tight">TitaouPosCRM</h1>
          <span class="px-2.5 py-0.5 bg-white/20 text-white rounded-full text-[10px] font-black uppercase tracking-wider backdrop-blur-xs">
            PRO EDITION
          </span>
        </div>
        <p class="text-xs text-sky-100 font-medium">Enterprise Desktop Point of Sale & Retail Management System</p>
        <p class="text-[11px] font-mono text-sky-200">{appVersion}</p>
      </div>
    </div>

    <!-- Quick Call Button -->
    <div class="flex items-center gap-3">
      <a
        href="tel:{developerPhone}"
        class="px-5 py-3 bg-white hover:bg-sky-50 text-sky-700 font-black text-xs rounded-2xl shadow-md flex items-center gap-2 transition cursor-pointer active:scale-95"
      >
        <Phone class="w-4 h-4 text-sky-600" />
        <span>Direct Call: {developerPhone}</span>
      </a>
    </div>
  </div>

  <!-- Content Grid: Developer Info & Technical Specs -->
  <div class="grid grid-cols-1 lg:grid-cols-3 gap-6 flex-1">
    <!-- 1. Lead Developer Profile -->
    <div class="bg-pos-card border border-pos-border rounded-3xl p-6 shadow-xs flex flex-col justify-between space-y-6">
      <div class="space-y-4">
        <div class="flex items-center gap-3 border-b border-pos-border pb-4">
          <div class="w-10 h-10 rounded-2xl bg-sky-100 dark:bg-sky-950 text-sky-600 flex items-center justify-center font-bold">
            <Info class="w-5 h-5" />
          </div>
          <div>
            <h2 class="text-sm font-black text-pos-text">Creator & Developer</h2>
            <p class="text-[11px] text-pos-muted">Architecture, Design & Engineering</p>
          </div>
        </div>

        <div class="p-4 bg-slate-50 dark:bg-slate-800/50 rounded-2xl border border-pos-border space-y-3">
          <div class="flex items-center justify-between text-xs">
            <span class="text-pos-muted font-bold">Lead Developer:</span>
            <span class="font-black text-pos-text">{developerName}</span>
          </div>
          <div class="flex items-center justify-between text-xs">
            <span class="text-pos-muted font-bold">Official Phone:</span>
            <span class="font-mono font-black text-sky-600">{developerPhone}</span>
          </div>
          <div class="flex items-center justify-between text-xs">
            <span class="text-pos-muted font-bold">Country / Region:</span>
            <span class="font-bold text-pos-text">Algeria (الجزائر)</span>
          </div>
          <div class="flex items-center justify-between text-xs">
            <span class="text-pos-muted font-bold">Support Availability:</span>
            <span class="font-bold text-emerald-600 flex items-center gap-1">
              <span class="w-2 h-2 rounded-full bg-emerald-500 animate-pulse"></span>
              24/7 Priority Support
            </span>
          </div>
        </div>
      </div>

      <!-- Social & Direct Support Channels -->
      <div class="space-y-2">
        <h3 class="text-xs font-black text-pos-text">Direct Contact Channels</h3>
        <div class="grid grid-cols-2 gap-2 text-xs">
          <button
            type="button"
            on:click={() => openExternal('https://wa.me/213553444057')}
            class="p-2.5 bg-emerald-50 hover:bg-emerald-100 dark:bg-emerald-950/40 text-emerald-700 dark:text-emerald-300 font-bold rounded-xl border border-emerald-200 dark:border-emerald-800 flex items-center justify-center gap-2 cursor-pointer transition"
          >
            <MessageCircle class="w-4 h-4 text-emerald-600" />
            <span>WhatsApp</span>
          </button>

          <button
            type="button"
            on:click={() => openExternal('https://web.facebook.com/AfakTech40')}
            class="p-2.5 bg-blue-50 hover:bg-blue-100 dark:bg-blue-950/40 text-blue-700 dark:text-blue-300 font-bold rounded-xl border border-blue-200 dark:border-blue-800 flex items-center justify-center gap-2 cursor-pointer transition"
          >
            <Facebook class="w-4 h-4 text-blue-600" />
            <span>Facebook</span>
          </button>

          <button
            type="button"
            on:click={() => openExternal('https://tiktok.com/@afaq_dz_official_?lang=en')}
            class="p-2.5 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 text-pos-text font-bold rounded-xl border border-pos-border flex items-center justify-center gap-2 cursor-pointer transition"
          >
            <Video class="w-4 h-4 text-slate-700 dark:text-slate-300" />
            <span>TikTok</span>
          </button>

          <button
            type="button"
            on:click={() => openExternal('https://www.youtube.com/@AfaqTechDZ')}
            class="p-2.5 bg-rose-50 hover:bg-rose-100 dark:bg-rose-950/40 text-rose-700 dark:text-rose-300 font-bold rounded-xl border border-rose-200 dark:border-rose-800 flex items-center justify-center gap-2 cursor-pointer transition"
          >
            <Video class="w-4 h-4 text-rose-600" />
            <span>YouTube</span>
          </button>

          <button
            type="button"
            on:click={() => openExternal('https://instagram.com/titaou.bedreddine')}
            class="p-2.5 bg-pink-50 hover:bg-pink-100 dark:bg-pink-950/40 text-pink-700 dark:text-pink-300 font-bold rounded-xl border border-pink-200 dark:border-pink-800 flex items-center justify-center gap-2 cursor-pointer transition"
          >
            <Instagram class="w-4 h-4 text-pink-600" />
            <span>Instagram</span>
          </button>

          <button
            type="button"
            on:click={() => openExternal('https://afaqtech.netlify.app/')}
            class="p-2.5 bg-slate-100 hover:bg-slate-200 dark:bg-slate-800 text-pos-text font-bold rounded-xl border border-pos-border flex items-center justify-center gap-2 cursor-pointer transition"
          >
            <Globe class="w-4 h-4 text-sky-600" />
            <span>Website</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 2. System Architecture & Capabilities -->
    <div class="bg-pos-card border border-pos-border rounded-3xl p-6 shadow-xs flex flex-col justify-between space-y-6 lg:col-span-2">
      <div class="space-y-4">
        <div class="flex items-center gap-3 border-b border-pos-border pb-4">
          <div class="w-10 h-10 rounded-2xl bg-emerald-100 dark:bg-emerald-950 text-emerald-600 flex items-center justify-center font-bold">
            <Cpu class="w-5 h-5" />
          </div>
          <div>
            <h2 class="text-sm font-black text-pos-text">Offline-First Desktop Architecture</h2>
            <p class="text-[11px] text-pos-muted">Engineered for zero-latency, high-reliability commercial retail</p>
          </div>
        </div>

        <div class="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div class="p-3.5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-1.5">
            <div class="flex items-center gap-2 text-xs font-black text-pos-text">
              <Database class="w-4 h-4 text-sky-600" />
              <span>Embedded SQLite 3</span>
            </div>
            <p class="text-[11px] text-pos-muted">Self-contained offline relational storage with ACID compliance, WAL journaling, and automated crash recovery.</p>
          </div>

          <div class="p-3.5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-1.5">
            <div class="flex items-center gap-2 text-xs font-black text-pos-text">
              <Layers class="w-4 h-4 text-emerald-600" />
              <span>Native ACLAS Scale SDK</span>
            </div>
            <p class="text-[11px] text-pos-muted">Direct binary synchronization over Ethernet TCP with UTF-16LE PLU generation and Type 97/02 barcodes.</p>
          </div>

          <div class="p-3.5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-1.5">
            <div class="flex items-center gap-2 text-xs font-black text-pos-text">
              <HardDrive class="w-4 h-4 text-purple-600" />
              <span>Direct Serial COM Cash Drawer</span>
            </div>
            <p class="text-[11px] text-pos-muted">Direct hardware pulse commands over COM1-10 with zero printer driver dependency or print dialog popups.</p>
          </div>

          <div class="p-3.5 bg-slate-50 dark:bg-slate-800/40 rounded-2xl border border-pos-border space-y-1.5">
            <div class="flex items-center gap-2 text-xs font-black text-pos-text">
              <ShieldCheck class="w-4 h-4 text-amber-600" />
              <span>Hardware-Locked Security</span>
            </div>
            <p class="text-[11px] text-pos-muted">Argon2 password hashing, perpetual offline licensing, and cryptographically verified transactions.</p>
          </div>
        </div>
      </div>

      <!-- Remote Diagnostics & Developer WhatsApp Support -->
      <div class="p-4 bg-slate-100 dark:bg-slate-800 rounded-2xl border border-pos-border flex flex-col md:flex-row items-center justify-between gap-4">
        <div>
          <h4 class="text-xs font-black text-pos-text">Remote Diagnostics & Developer Support</h4>
          <p class="text-[11px] text-pos-muted">Copy current system environment snapshot to send directly to developer <strong>Titaou Bedreddine</strong>.</p>
        </div>
        <div class="flex items-center gap-2 shrink-0">
          <button
            type="button"
            on:click={() => {
              const diag = `[TitaouPosCRM Diag Snapshot]\nVersion: ${appVersion}\nDeveloper: ${developerName} (${developerPhone})\nTime: ${new Date().toISOString()}\nPlatform: Windows x64 Offline`;
              navigator.clipboard.writeText(diag);
              alert('✅ Diagnostic snapshot copied to clipboard!');
            }}
            class="px-3.5 py-2 bg-pos-card hover:bg-slate-200 dark:hover:bg-slate-700 text-pos-text border border-pos-border font-bold text-xs rounded-xl cursor-pointer transition shadow-2xs"
          >
            Copy Diagnostics
          </button>

          <button
            type="button"
            on:click={() => {
              const msg = encodeURIComponent(`Bonjour Titaou, j'utilise TitaouPosCRM (${appVersion}) et j'ai besoin d'assistance.`);
              openExternal(`https://wa.me/213${developerPhone.slice(1)}?text=${msg}`);
            }}
            class="px-4 py-2 bg-emerald-600 hover:bg-emerald-700 text-white font-black text-xs rounded-xl cursor-pointer transition shadow-xs flex items-center gap-1.5"
          >
            <MessageCircle class="w-3.5 h-3.5" />
            <span>Send to Developer</span>
          </button>
        </div>
      </div>
    </div>
  </div>
</div>
