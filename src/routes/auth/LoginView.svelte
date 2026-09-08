<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '../../lib/i18n';
  import { currentUser } from '../../lib/stores/auth';
  import { networkStatus } from '../../lib/stores/network';
  import type { User } from '../../lib/types';
  import { Lock, ChevronDown, Eye, EyeOff, X, Wifi, WifiOff, Loader2 } from 'lucide-svelte';

  let usersList: User[] = [];
  let selectedUsername = 'admin';
  let password = '';
  let showPassword = false;
  let errorMsg = '';
  let isLoading = false;
  let appVersion = '';
  let ipcDead = false;

  // IPC watchdog: ping a cheap command on an interval. The webview<->Rust
  // transport can silently wedge at boot; two consecutive missed pings
  // (4s each) means login can never succeed, so offer a one-click Reload.
  let passwordInputEl: HTMLInputElement;

  // RFID badge / employee QR scan on the LOGIN screen: the card IS the
  // credential — a valid tag logs the linked user in automatically.
  let rfidBuffer = '';
  let rfidLastKey = 0;
  let rfidFlash = '';

  async function tryRfidLogin(raw: string): Promise<boolean> {
    const tag = raw.trim();
    if (!tag) return false;
    const isTag = tag.startsWith('RFID-') || tag.startsWith('EMP-QR-') || tag.startsWith('EMP_');
    if (!isTag) return false;
    try {
      const user = await invoke<User | null>('login_with_rfid', { rfid: tag });
      if (user) {
        rfidFlash = '';
        currentUser.set(user);
        $currentUser = user;
        return true;
      }
      rfidFlash = '❌ Unknown card / بطاقة غير معروفة';
      setTimeout(() => (rfidFlash = ''), 3000);
    } catch (e) {
      console.error('RFID login failed:', e);
    }
    return false;
  }

  function handleGlobalScan(e: KeyboardEvent) {
    // Ignore keystrokes while the password box is being used — the watcher
    // only captures fast scanner bursts when no field has focus.
    const active = document.activeElement as HTMLElement | null;
    if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) return;
    const now = Date.now();
    if (now - rfidLastKey > 100) rfidBuffer = '';
    rfidLastKey = now;
    if (e.key === 'Enter') {
      const candidate = rfidBuffer;
      rfidBuffer = '';
      if (candidate.length >= 4) {
        void tryRfidLogin(candidate);
      }
      return;
    }
    if (e.key.length === 1) rfidBuffer += e.key;
  }

  onMount(() => {
    password = '';
    // Cursor starts in the password box so the user can type straight away.
    setTimeout(() => passwordInputEl?.focus(), 100);
    let missedPings = 0;
    const watchdog = setInterval(async () => {
      try {
        await Promise.race([
          invoke('get_app_version'),
          new Promise((_, reject) => setTimeout(() => reject(new Error('ping timeout')), 4000)),
        ]);
        missedPings = 0;
        ipcDead = false;
      } catch {
        missedPings++;
        if (missedPings >= 2) ipcDead = true;
      }
    }, 2000);
    window.addEventListener('keydown', handleGlobalScan);
    return () => {
      clearInterval(watchdog);
      window.removeEventListener('keydown', handleGlobalScan);
    };
  });

  onMount(async () => {
    try {
      appVersion = await invoke<string>('get_app_version');
    } catch (e) {
      appVersion = '';
    }
    try {
      usersList = await invoke<User[]>('get_active_users');
      if (usersList && usersList.length > 0) {
        selectedUsername = usersList[0].username;
      } else {
        selectedUsername = 'admin';
      }
    } catch (e) {
      console.error('Failed to load users:', e);
      usersList = [{ id: 1, username: 'admin', display_name: 'Administrator', role_id: 1, role_name: 'Administrator', max_discount_percent: 100, is_active: true, permissions: [] }];
      selectedUsername = 'admin';
    }
  });

  async function handleLogin() {
    const uname = (selectedUsername || 'admin').trim();
    const pwd = (password || '').trim();

    if (!uname || !pwd) {
      errorMsg = 'Please enter password / الرجاء إدخال كلمة المرور';
      return;
    }
    try {
      isLoading = true;
      errorMsg = '';
      // Hard timeout: the login backend can take ~1s (Argon2) but must
      // never hang the button forever — if it does, surface it instead of
      // spinning, so the cashier can retry or restart.
      const user = await Promise.race([
        invoke<User | null>('login', { username: uname, password: pwd }),
        new Promise<null>((resolve) => setTimeout(() => resolve(null), 15000)),
      ]);
      if (user) {
        password = '';
        currentUser.set(user);
        $currentUser = user;
      } else {
        errorMsg = 'Invalid password / كلمة المرور غير صحيحة';
      }
    } catch (err: any) {
      console.error('Login error:', err);
      errorMsg = typeof err === 'string' ? err : err.message || 'Login failed';
    } finally {
      isLoading = false;
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      handleLogin();
    }
  }
</script>

<div class="h-screen w-screen bg-slate-950 flex items-center justify-center p-4 select-none">
  <div class="bg-slate-900 border border-slate-800 rounded-3xl p-8 max-w-md w-full shadow-2xl space-y-6">
    <div class="text-center space-y-2">
      <div class="w-16 h-16 rounded-2xl bg-white flex items-center justify-center mx-auto shadow-lg shadow-sky-600/20 overflow-hidden p-1 border border-slate-700">
        <img src="/logo.png" alt="TitaouPOS" class="w-full h-full object-contain" />
      </div>
      <div class="flex items-center justify-center gap-2">
        <h1 class="text-2xl font-black text-white tracking-tight">TitaouPOS</h1>
        {#if appVersion}
          <span class="px-2 py-0.5 bg-sky-950 border border-sky-800 text-sky-300 text-[10px] font-mono font-bold rounded-full">
            v{appVersion}
          </span>
        {/if}
      </div>
      <p class="text-xs text-sky-400 font-bold">Created by Titaou Bedreddine 0553444057</p>
    </div>

    {#if errorMsg}
      <div class="p-3 bg-rose-950/60 border border-rose-800 text-rose-300 text-xs font-bold rounded-xl text-center">
        {errorMsg}
      </div>
    {/if}

      {#if rfidFlash}
        <p class="text-[11px] font-bold text-rose-400 text-center">{rfidFlash}</p>
      {/if}
      <p class="text-[10px] text-slate-500 text-center">{t('login_rfid_hint')}</p>

    <div class="space-y-4">
      <!-- User Selection Dropdown -->
      <div>
        <label class="block text-xs font-bold text-slate-400 mb-1">{t('login_username')}</label>
        <div class="relative">
          <select
            bind:value={selectedUsername}
            on:change={() => { password = ''; passwordInputEl?.focus(); }}
            class="w-full px-4 py-3 bg-slate-800 border border-slate-700 rounded-xl text-sm font-bold text-white outline-none focus:border-sky-500 transition appearance-none cursor-pointer"
          >
            {#if usersList.length === 0}
              <option value="admin">Administrator (Administrator)</option>
            {:else}
              {#each usersList as u}
                <option value={u.username}>
                  {u.display_name} ({u.role_name || u.username})
                </option>
              {/each}
            {/if}
          </select>
          <div class="absolute inset-y-0 end-0 flex items-center px-3 pointer-events-none text-slate-400">
            <ChevronDown class="w-4 h-4" />
          </div>
        </div>
      </div>

      <div>
        <div class="flex items-center justify-between mb-1">
          <label class="block text-xs font-bold text-slate-400">{t('login_password')}</label>
          <button
            type="button"
            on:click={() => (showPassword = !showPassword)}
            class="text-[11px] text-slate-400 hover:text-sky-400 font-bold flex items-center gap-1 cursor-pointer"
          >
            {#if showPassword}
              <EyeOff class="w-3.5 h-3.5" />
              <span>Hide</span>
            {:else}
              <Eye class="w-3.5 h-3.5" />
              <span>Show</span>
            {/if}
          </button>
        </div>
        <div class="relative">
          <input
            type={showPassword ? 'text' : 'password'}
            bind:value={password}
            bind:this={passwordInputEl}
            autocomplete="new-password"
            placeholder="••••••••"
            autofocus
            on:keydown={handleKeyDown}
            class="w-full px-4 py-3 pe-10 bg-slate-800 border border-slate-700 rounded-xl text-sm font-bold text-white outline-none focus:border-sky-500 transition font-mono"
          />
          {#if password}
            <button
              type="button"
              on:click={() => { password = ''; passwordInputEl?.focus(); }}
              class="absolute inset-y-0 end-3 flex items-center text-slate-400 hover:text-white transition cursor-pointer"
              title="Clear password"
            >
              <X class="w-4 h-4" />
            </button>
          {/if}
        </div>
      </div>

      {#if $networkStatus?.enabled && ($networkStatus?.role === 'client' || $networkStatus?.mode === 'connected')}
      <div
        class="p-2.5 rounded-xl border text-[11px] font-bold flex items-center gap-2
        {$networkStatus?.mode === 'connected'
          ? 'bg-emerald-950/60 border-emerald-800 text-emerald-300'
          : 'bg-amber-950/60 border-amber-800 text-amber-300'}"
      >
        {#if $networkStatus?.mode === 'connected'}
          <Wifi class="w-4 h-4 shrink-0" />
          <span>Connected to shop server {$networkStatus?.coordinator?.pc_name ? `(${$networkStatus.coordinator.pc_name})` : ''} — sign in with your shop account.</span>
        {:else if $networkStatus?.mode === 'searching' || $networkStatus?.mode === 'reconnecting'}
          <Loader2 class="w-4 h-4 shrink-0 animate-spin" />
          <span>Searching for the shop server… login unlocks automatically once connected / جارٍ البحث عن الخادم…</span>
        {:else}
          <WifiOff class="w-4 h-4 shrink-0" />
          <span>Shop server unreachable — login is blocked on this terminal until the connection returns / لا يمكن تسجيل الدخول قبل الاتصال بالخادم</span>
        {/if}
      </div>
    {/if}

    {#if ipcDead}
        <div class="p-3 bg-rose-100 text-rose-800 text-xs font-bold rounded-xl flex items-center justify-between gap-2">
          <span>Backend connection stuck — click Reload / انقطع الاتصال الداخلي</span>
          <button
            type="button"
            on:click={() => window.location.reload()}
            class="px-3 py-1.5 bg-rose-600 text-white text-xs font-black rounded-lg cursor-pointer shrink-0"
          >
            Reload
          </button>
        </div>
      {/if}

      <button
        type="button"
        on:click={handleLogin}
        disabled={isLoading}
        class="w-full py-3.5 bg-sky-600 hover:bg-sky-700 disabled:opacity-50 text-white font-black text-sm rounded-xl transition shadow-lg shadow-sky-600/30 flex items-center justify-center gap-2 cursor-pointer"
      >
        <Lock class="w-4 h-4" />
        <span>{isLoading ? 'Signing In...' : 'Sign In to POS / تسجيل الدخول'}</span>
      </button>
    </div>
  </div>
</div>