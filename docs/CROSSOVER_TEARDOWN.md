# CrossOver 26.3.0 for macOS — Teardown Blueprint

> **مصدر هذا المستند:** تفكيك ساكن (static) لنسخة CrossOver 26.3.0 Demo على macOS،
> نزّلناها وفككناها في `E:\rea_lab\cxmac\CrossOver.app`، وحللناها بـ REA 6.7.0 +
> Ghidra 12.1.4 + radare2 5.9.8 + Python 3.13 bytecode tools، وبـ 12 وكيل RE متوازٍ.
> **التقرير الموحّد:** `E:\rea_lab\reports\00_MASTER.md` — والتقارير التفصيلية `01..12_*.md` بنفس المجلد.
>
> **مبدأ:** لا ننسخ كود CodeWeavers (مغلق تجاريًا). نبني **أنظمة مماثلة** من
> معماريتهم المفهومة + مكوّنات مفتوحة المصدر (Wine LGPL, DXVK zlib, DXMT, MoltenVK
> Apache-2.0, vkd3d). الجزء الوحيد المغلق (**D3DMetal** من Apple، مرخّص لـ
> CodeWeavers) **لا يُنسخ ولا يُوزَّع**.

---

## 1) خريطة الأنظمة كما فهمها من الكود

| # | نظام CrossOver | أين يعيش | ما يفعله | حالتنا |
|---|---|---|---|---|
| 1 | **GUI shell** | `Contents/MacOS/CrossOver` (ObjC، 88 صنفاً، 5,215 طريقة) | Cocoa UI + جسر PyObjC | — |
| 2 | **منطق المنتج** | `SharedSupport/.../lib/python/*.pyc` (50 مودول، CPython 3.13 مدمج) | القوارير، التثبيت، البروفايلات، الترخيص | ✅ `c4p.rs` · `compat.rs` · `install_plan.rs` |
| 3 | **محرّك Unix** | `CrossOver-Hosted Application/*` (21 سكربت Perl + ثنائيات) | تشغيل Wine، القوائم، الجمعيات، الفحص | `steam.rs`, `graphics.rs` |
| 4 | **محرك Wine** | `lib/wine/{x86_64-unix, x86_64-windows, i386-windows}` | Wine **11.0** (`wine-11.0-8726-g2e2f5fca349`) · **x86_64 فقط ⇒ Rosetta إلزامي** | Gcenx 11.18 (أحدث) |
| 5 | **طبقة D3D** | `lib/dxmt`, `lib/dxvk`, `lib64/libMoltenVK.dylib`, `lib64/apple_gptk/D3DMetal.framework` | 5 مسارات | `graphics.rs` (DXVK/WineD3D) |
| 6 | **طبقة العرض** | `lib/wine/x86_64-unix/winemac.so` | macdrv + رقع `my_*` + Metal view/layer | ⬜ Port مطلوب |
| 7 | **قواعد التوافق** | `lib/wine/x86_64-unix/cxcompatdb.so` + `compatdb-N.dat` | hacks لكل تطبيق داخل Wine | ⬜ `compat_rules.rs` |
| 8 | **قاعدة البروفايلات** | `share/crossover/data/crossover.tie` (23.7MB) | 5,308 تطبيق · 90,890 medal · 4,601 install profile · 1,828 بروفايل بـ`steamid` | ✅ `c4p.rs` |
| 9 | **قوالب القوارير** | `share/crossover/bottle_templates/*` (**13**) | win98…win11_64 | `bottle.rs` |
| 10 | **الترخيص** | `demoutils` + `fileupdate` + `tie.pub` | RSA‑2048 + خادم codeweavers | ⬜ بلا ترخيص (MIT) |

---

## 2) خمسة اكتشافات تغيّر خارطة الطريق

### 2.1 الهجين ObjC + Python داخل نفس العملية
`_InitializePython` (مفكّك): `PYTHONPATH=<root>/lib/python` → `Py_InitializeEx(0)` →
`PyImport_ImportModule("cxutils")` →-set `cxutils.CX_ROOT` → `PyRun_SimpleFileExFlags(libcxsetupbase.py)` →
`libcxsetup.py`. وبعدها `CXSetupLib::initialize` يبحث عن أصناف Python عبر `NSClassFromString`
(`BottleQuery`, `BottleWrapper`, `DemoUtils`, `Engine`, `InstallTask`, …).
**الدلالة:** فصل «عرض» عن «منطق» — نحن: Rust للمنطق + WebView للواجهة ⇒ نفس الفصل.

### 2.2 اختيار محرك الرسوميات لكل تطبيق يتم **داخل Wine**
`cxcompatdb.so` (مفكّك 267KB) يقرأ `compatdb-N.dat` (JSON مشفّر) ويطبّق hacks:
`graphics_backend` · `dll_overrides` · `append_cmd_line` · `env_vars` · `replace_exe_path`،
المطابقة بـ`steam_game_id` أو `path_suffix`، ويكتب `CX_GRAPHICS_BACKEND` /
`CX_ACTIVE_GRAPHICS_BACKEND` داخل عملية Wine.
**الدلالة:** بدل تبديل DLLs يدوياً (كما نفعل)، الأنظف: قاعدة بروفايل لكل تطبيق + متغير بيئة.

### 2.3 رقع Metal في `winemac.so` — الحل الجذري للشاشة السوداء
macdrv العادي + دوال CodeWeavers:
```
my_macdrv_create_metal_device / my_macdrv_release_metal_device
my_macdrv_view_create_metal_view / my_macdrv_view_get_metal_layer
my_macdrv_view_release_metal_view / my_macdrv_get_cocoa_window
my_macdrv_init_display_devices
أصنافهم: WineMetalView (initWithFrame:device:, setRetinaMode:, makeBackingLayer)
          WineMetalLayer : CAMetalLayer  ← يعترض nextDrawable ويمرّر d3dmetal_client_surface
                          إلى خادم Wine قبل الاستدعاء الأصلي
شيم Win32: my_GetSystemMetrics, my_EnumDisplayMonitors, my_GetMonitorInfoA,
            my_AdjustWindowRectEx, my_SetWindowPos, my_MoveWindow, …
شيم ريجستري: my_RegOpenKeyExA, my_RegCreateKeyExA, my_RegQueryValueExA, my_RegSetValueExA
Retina: جدولان [2.0,1.0] و[0.5,2.0] ⇒ تصغير 0.5× + contentsScale=2.0 + kCAFilterLinear
```
في Gcenx 11.18 الدالة المقابلة `macdrv_client_surface_acquire_metal_swapchain`
موجودة كـ **FIXME** ⇒ نافذة child (CEF) لا تُعرض.
**الخيار الحقيقي بعيد المدى:** port فكرة `WineMetalView/WineMetalLayer` إلى Gcenx
(وليس CEF wrapper). effort: أسبوعان، النتيجة: كل نوافذ child تُعرض Metal.

### 2.4 D3DMetal = Apple GPTK 3.0 ⇒ macOS 15.4+
`com.apple.D3DMetal` · D3DRendererMetal‑32.47 · SDK 15.4 · D3D11+D3D12 → Metal ·
**غير قابل للتوزيع** (مرخّص لـ CodeWeavers). ⇒ وعدنا بـ«macOS 11+» يستلزم الاعتماد على
DXMT/vkd3d لا D3DMetal. (DXMT عندهم v0.72، D3D11 فقط، مع backend `airconv` → Apple AIR.)

### 2.5 WoW64 حديث + 16-بت
`wow64.dll` (`Wow64ShallowThunk*`) · `wow64cpu.dll` (`BTCpuSimulate`) · `wow64win.dll` في
`x86_64-windows` فقط، ومجلد `i386-windows` فيه 835 PE (منها 60 ملف 16-بت + `winevdm.exe`)
⇒ WoW64 بنمط **Binary Translation** لا wow64new على x86_64.

---

## 3) تصحيحات على الافتراضات السابقة (تحقّق شخصي)

| الافتراض القديم | الصحيح | كيف تحقّق |
|---|---|---|
| «CrossOver يشحن DXVK 1.5.1» | ❌ **ليس 1.5.1** — MD5 لكل DLLs لا يطابق جدول 1.5.1 ⇒ نسخة أحدث | hashes على الملفات الفعلية |
| «12 قالب قارورة» | ❌ **13** (win98…win11_64، 6×64-بت) | `difflib` على `CXBT_*.pm` |
| «Hardened Runtime غير مفعّل» | ❌ **مفعّل** بعلم `CS_RUNTIME_NOLIBVALIDATION` (0x10000) بلا library validation | قراءة CodeDirectory من LC_CODE_SIGNATURE |
| «وسام الـ medal سمة» | ❌ **نص العنصر**: `<medal rating="5" …>gold</medal>` | c4parser + XML |
| «crossover.tie فيه 5,308 apps» | ✅ صحيح، **وإضافة**: 1,828 بروفايل فيه `steamid` | SQLite faithful |

---

## 4) ما بنيناه داخل `jisr`

| الملف | ماذا | اختبارات |
|---|---|---|
| `src-tauri/src/c4p.rs` | محرك بروفايلات التوافق (صيغة c4p/tie) — streaming، بحث، medals، إبطال، منع path traversal | **10 ✅** (+ على قاعدة CrossOver الحقيقية 23MB في **2.5s**) |
| `src-tauri/src/install_plan.rs` | مخطّط تثبيت (AIE-equivalent): رسم graph، كشف الدورات، ترتيب تنفيذي حتمي | **11 ✅** |
| `src-tauri/src/compat.rs` | مخزن البروفايلات عندنا + import + أوامر Tauri | **6 ✅** |
| `docs/CROSSOVER_TEARDOWN.md` | هذا المستند | — |

**البوابات:** `cargo fmt` ✅ · `cargo test --lib` → **130 passed / 0 failed / 3 ignored** ✅ · `cargo check` ✅

### 4.1 كيف نستعمل `c4p`
```rust
let db = c4p::load(Path::new("crossover.tie"))?;   // streaming
db.search("half-life");                             // id/اسم/تصنيف
db.medal_for("com.codeweavers.c4.3", "Mac");         // gold > silver > bronze (ungold = سالب)
db.is_revoked(appid, "2026-10-21");                  // نافذة زمنية
```
### 4.2 كيف نستعمل `install_plan`
```rust
let plan = install_plan::plan(&PlanRequest {
    apps: vec![/* AppRequest{ dependencies, download, installer_command, … } */],
    bottle_exists: false,
    needs_media: true,
})?;   // PlanError::Cycle | UnknownDependency | DuplicateApp
for task in plan.tasks { /* نفّذ حسب TaskKind::label */ }
```
### 4.3 كيف نستعمل `compat`
مجلد `~/Library/Application Support/Jisr/compat/` يحتوي ملفات `.c4p` (صيغتنا)؛
`compat_import` يتحقق من الملف قبل نسخه؛ `compat_search` يرجّع الاسم بالعربية + medal + القالب + الاعتماديات.

---

## 5) خارطة التنفيذ (بعد هذا الـ RE)

| # | الشريحة | الملفات | الوقت | الأولوية |
|---|---|---|---|---|
| A | **قاعدة توافقنا** (`jisrdb` XML): `steamid` + `path_suffix` + hacks (graphics_backend/env_vars/append_args/dll_overrides) — **خارج Wine** | `compat_rules.rs` | 2–3 س | 🔥 عالية |
| B | **13 قالب قارورة** (`bottle_template.rs`): win98…win11_64 + `system.reg` + `run_inf` + Wow64 | `bottle_template.rs` | 2 س | 🔥 عالية |
| C | **تنفيذ الـ hacks**: كتابة `CX_GRAPHICS_BACKEND`/`CX_ACTIVE_GRAPHICS_BACKEND` + `WINEDLLOVERRIDES` | `compat_rules.rs` | 2 س | متوسطة |
| D | **ورقة Metal**: port `WineMetalView/WineMetalLayer` لمحرّك Gcenx (PR لمشروع Wine) | خارج المستودع | 1–2 أسبوع | 🔥 عالية (يبطّئ كل شيء) |
| E | **تنفيذ `TaskKind`** على الأرض (تنزيل، فك، تشغيل، ريجستري، DLLs) | `install_exec.rs` | 3–4 س | متوسطة |
| F | **تحديث `docs/COMPARISON.md`** بالأرقام الجديدة (5 مسارات رسومية، WoW64 حديث، D3DMetal 15.4+) | `docs/` | 1 س | سريعة |

---

## 6) حدود هذا الـ RE

- تحليل ساكن فقط (لا macOS ⇒ لا dynamic analysis).
- مزوّد REA على Windows لا يدعم Mach-O؛ استُخدم Ghidra 12.1.4 مباشرة + radare2 + Python 3.13.
- قوالب `crossover.tie` نُسخت **للبحث فقط**؛ قاعدة التوزيع عندنا بصيغتنا (نفس المخطط) ومملوكة لنا.
- لم يُكسر أي ترخيص ولم تُعدَّل أي حماية؛ التحليل لأغراض التوافقية (interoperability) وفهم البنية.

---

## 7) جولة البناء الثانية — «نسخ طريقةهم» (2026-10-21)

### 7.1 ما الذي ننسخه وما الذي لا ننسخه
| المكوّن عند CrossOver | قرارنا | كيف |
|---|---|---|
| Wine 11.0 · DXVK · DXMT · MoltenVK · vkd3d-proton · msync · Sparkle | ✅ **نسخ من المصدر** بنفس الإصدارات | `vendor/fetch-sources.sh` — ملفات الترخيص تبقى داخل الشجرة (شرط LGPL/Apache/GPL) |
| بايت-كود Python (50 مودول) | ❌ مملوك لهم ⇒ **نعيد كتابة الأنظمة** | `c4p` · `compat` · `install_plan` · `graphics_backend` · `compat_rules` · `bottle_template` |
| `crossover.tie` (23.7MB) | ❌ بيانات مملوكة ⇒ **قاعدتنا** بنفس المخطط | `jisrdb` + `resources/compat/default.c4p` |
| D3DMetal (Apple GPTK 3.0) | ❌ redistribution ممنوع ⇒ **المستخدم يثبّت GPTK من Apple** | `graphics_backend::d3dmetal_framework_candidates()` يكتشف المثبّت ويستخدمه |
| رقع Metal في `winemac.so` (`my_*`) | ❌ كودهم ⇒ **نكتبه من الصفر** لمحرّك Gcenx | PR لمشروع Wine (ورقة Metal) |

> **ملاحظة قانونية عملية:** شيل ملفات الترخيص من LGPL/GPL/Apache = خرق من أول سطر،
> وLGPL تفرض إتاحة السورس (شجرة `vendor/src` هي نفس ما نوزّعه ⇒ أسهل الالتزام).
> المستودع يبقى صالح للنشر العام، والتحديث التلقائي(minisign) يشتغل.

### 7.2 الوحدات الجديدة
| الوحدة | تقابل عندهم | اختبارات |
|---|---|---|
| `graphics_backend.rs` | `bottlewrapper` (اختيار الرسوميات لكل قارورة) + `upgrade_graphics_settings` | 9 ✅ |
| `compat_rules.rs` | `cxcompatdb.so` (قواعد لكل تطبيق) — **خارج Wine** عندنا | 8 ✅ |
| `bottle_template.rs` | `CXBT_*.pm` (13 قالب) + `run_inf` + `cxbottle.conf` | 12 ✅ |

**المجموع:** `cargo test --lib` → **161 passed / 0 failed / 3 ignored** · `cargo fmt --check` ✅ · `cargo check` ✅

---

## 8) النظام النهائي عندنا — خريطة الأنظمة

| نظام | عند CrossOver | عندنا | الملف | الحالة |
|---|---|---|---|---|
| محرك الرسوميات لكل قارورة | `bottlewrapper` + `upgrade_graphics_settings` | اختيار `auto/dxvk/wined3d/dxmt/d3dmetal` + ريجستري `renderer=vulkan` + `DllOverrides` + `CX_GRAPHICS_BACKEND` | `graphics_backend.rs` | ✅ |
| القواعد لكل لعبة | `cxcompatdb.so` (داخل Wine) | نفس الـhacks، **خارج Wine**، بصيغة XML | `compat_rules.rs` | ✅ |
| قاعدة البروفايلات | `crossover.tie` (23.7MB، 5,308 تطبيق) | `jisrdb` بصيغة c4p + بذورنا + استيراد/تصدير | `c4p.rs` · `compat.rs` | ✅ |
| محرّك التثبيت | `AIECore` (28 خطوة) | مخطّط + تنفيذ: تنزيل ببصمة، كاش، فك، تشغيل، ريجستري، reboot | `install_plan.rs` · `install_exec.rs` | ✅ |
| قوالب القوارير | 13 × `CXBT_*.pm` | 13 قالب + خطة إنشاء (INF مرّتين لـ Wow64) | `bottle_template.rs` | ✅ |
| إعدادات القارورة | `cxbottle.conf` | `<prefix>/crossover.conf` (طبقة INI عامة) | `bottle_settings.rs` | ✅ |
| الفحص والإصلاحات | `cxdiag` + `cxfixes.xml` | 6 فحوص (GStreamer/32-bit/renderer/DXVK/Rosetta/msync) بقواعد XML | `fixes.rs` | ✅ |
| اختصارات macOS | `cxmenu` + `cxassoc` | بناء `.app` من مدخلات Start Menu + تسجيل LaunchServices | `menu_sync.rs` | ✅ |
| مساعد القارورة (Apple Events) | "Bottle Helper.cpbz2" | تطبيق Swift + سكربت بناء + أمر تثبيت | `resources/bottle-helper/` | ✅ |
| الترخيص | RSA + خادم codeweavers | **لا شيء** — مجاني بالكامل | — | ✅ |
| طبقة Metal | رقع `my_*` في winemac.so | ورقة `METAL-PORT.md` (التشخيص مُثبَت من سورس Wine) | `docs/METAL-PORT.md` | 📄 |

### الأوامر الجديدة (Tauri)
```
graphics_backend_get / graphics_backend_set / graphics_backends / d3dmetal_present
compat_stats / compat_search / compat_get / compat_import / compat_export
rules_list / rules_save
fixes_check / fixes_set_msync
menu_sync_bottle / helper_install
install_preview / install_run
```

### البوابات
```
cargo fmt --check ✔ · cargo test --lib → 191 passed / 0 failed ✔ · cargo check ✔
npx tsc --noEmit ✔ · npm run build ✔ · vendor/check-licenses.sh ✔
CI لكل PR: .github/workflows/ci.yml (Rust + الواجهة + بوابة التراخيص)
```
