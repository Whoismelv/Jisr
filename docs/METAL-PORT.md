# ورقة Metal: نوافذ child (واجهة ستيم) سوداء — الحل الحقيقي

> **مستند تقني مبني على فحص فعلي لسورس Wine** (نزّلنا `dlls/winemac.drv/` من وسوم
> `wine-11.0` و`wine-11.2/5/8/10/12/15/18/20` ومن `master`، وقرأنا الكود سطر سطر)
> + تفكيك `winemac.so` من CrossOver 26.3.0 (Ghidra).
> **التاريخ:** 2026-10-22 · **الحالة:** جاهز للتنفيذ

---

## 1) التشخيص: أين بالضبط ينكسر

في Wine master (وهو 12.0-dev)، الملف `dlls/winemac.drv/window.c` سطر 1159:

```c
bool macdrv_client_surface_acquire_metal_swapchain(struct macdrv_client_surface *surface)
{
    HWND hwnd = surface->client.hwnd;
    struct macdrv_win_data *data;

    if (surface->metal_swapchain) return true;

    if ((data = get_win_data(hwnd)))            /* النافذة في عمليتنا */
    {
        release_win_data(data);
        surface->metal_swapchain = macdrv_create_view_swapchain(surface->cocoa_view);   /* ✅ */
    }
    else
    {
        RECT rect;
        if (NtUserGetAncestor(hwnd, GA_ROOT) != hwnd)
        {
            FIXME("Cross-process child window Metal swapchains are not implemented\n");  /* ❌ */
            return false;
        }
        if (!NtUserGetClientRect(hwnd, &rect, ...)) return false;
        surface->metal_swapchain = macdrv_create_offscreen_swapchain(hwnd, cgrect_from_rect(rect)); /* ✅ */
    }
    return surface->metal_swapchain != NULL;
}
```

**المستهلك الوحيد** — `dlls/winemac.drv/vulkan.c:44`:

```c
static VkResult macdrv_vulkan_surface_create(struct client_surface *client, ...)
{
    if (!macdrv_client_surface_acquire_metal_swapchain(surface))
        return VK_ERROR_INCOMPATIBLE_DRIVER;      /* ←_surface creation fails */
    create_info_host.sType = VK_STRUCTURE_TYPE_METAL_SURFACE_CREATE_INFO_EXT;
    create_info_host.pLayer = macdrv_swapchain_get_layer(surface->metal_swapchain);
    ...
}
```

### سلسلة الفشل كاملة
```
لعبة/ستيم يفتح نافذة child (CEF) داخل نافذة ستيم
   └─ child في عملية أخرى  → get_win_data(hwnd) == NULL
        └─NtUserGetAncestor(hwnd, GA_ROOT) != hwnd  → FIXME → return false
             └─ macdrv_vulkan_surface_create → VK_ERROR_INCOMPATIBLE_DRIVER
                  └─ MoltenVK ما عنده Metal surface → لا presentation
                       └─ نافذة سوداء  ❌  (وهذا بالضبط ما تشوفه)
```

---

## 2) متى أُضيف هذا الكود؟ (فحص على الوسوم — مهم)

| وسم Wine | `acquire_metal_swapchain` | `macdrv_create_view_swapchain` | FIXME للـ child |
|---|---|---|---|
| wine-11.0 | ❌ | ❌ | — |
| wine-11.2 / 11.5 / 11.8 / 11.10 | ❌ | ❌ | — |
| **wine-11.12** | ✅ | ✅ | ✅ (FIXME موجود) |
| wine-11.15 / 11.18 / 11.20 | ✅ | ✅ | ✅ |
| master (12.0-dev) | ✅ | ✅ | ✅ (ما زال) |

**النقطتان:**
1. **أُضيف Metal swapchain للنوافذ العادية في Wine 11.12** — أي لو محركك Gcenx
   مبني على **Wine 11.0** (أقدم من 11.12) فأنت **ما عندك المسار Metal أصلاً** للنوافذ
   ⇒ لا DXVK+MoltenVK لنافذة child ولا لنافذة عادية. ⇒ **أول خطوة: تأكد إن محركك
   مبني على 11.12+** (`wine-devel-11.18` من Gcenx = Wine 11.18 ✅).
2. **حالة child عبر العمليات ما زالت FIXME حتى اليوم في upstream** — يعني: **ما فيه
   حل جاهز في أي Wine released** ⇒ إما نعمل fork، أو نتجاهل المسار.

---

## 3) كيف حلّته CrossOver (من تفكيك `winemac.so`)

CrossOver لا يسلّك الطريق أعلاه؛ هم **رقعوا macdrv** ودخلوا طبقة Metal بأنفسهم:

```
my_macdrv_create_metal_device        → يضمن جهاز Metal قبل العرض
my_macdrv_release_metal_device
my_macdrv_view_create_metal_view     → ينشئ WineMetalView (CAMetalLayer) لنافذة child
my_macdrv_view_get_metal_layer
my_macdrv_view_release_metal_view
my_macdrv_get_cocoa_window            → يجد NSWindow负责人
my_macdrv_init_display_devices
```
وأهمهم: `WineMetalLayer : CAMetalLayer` **يعترض `nextDrawable`** ويرسل قبل الاستدعاء
الأصلي طلباً إلى **wineserver** باسم `d3dmetal_client_surface` — أي **يمرّر طبقة
Metal الخاصة بالنافذة(child) إلى العملية التي تملك الـ swapchain**، فتتمكّن من الرسم فيها.

**نتيجة:** نافذة child (واجهة ستيم) تُعرض Metal ⇒ لا شاشة سوداء.

---

## 4) خطة التنفيذ عندنا — ثلاث طبقات مرتّبة بالتكلفة

### الطبقة A — بلا fork (جرّبها أول شي، 10 دقائق)
1. **تأكد من إصدار Wine في محركك**: لازم **11.12+** (يفحص `lib/wine/x86_64-unix/ntdll.so` عن سلسلة `wine-11.x`).
2. **خلّي نافذة CEF في نفس العملية**: `--single-process` (مفعّل عندك افتراضياً في `cef-flags.txt`).
   لأن الـ FIXME يُطلق **فقط** لما تكون النافذة في عملية أخرى.
3. **خلّي CEF يستخدم Vulkan بدل software**: `--in-process-gpu --use-angle=vulkan`
   (سطر موجود أصلاً في ملاحظاتك).此时 المسار = Vulkan → MoltenVK → `VK_EXT_metal_surface`
   → `macdrv_create_view_swapchain` ✅ (نفس العملية ⇒ ينجح).
4. **لا تستخدم `--disable-gpu`** إلا اضطراراً (يمسح Metal كله ⇒ software rendering).

**اختبار:** شغّل ستيم، افتح «المكتبة». لو الواجهة ظهرتMetal-smooth ⇒ الطبقة A نجحت.

### الطبقة B — Metalswapchain لـchild عبر العمليات (patch حقيقي)
المطلوب في `dlls/winemac.drv/` + طبقة الخادم:

```
1) server/window.c: طلب جديد
   set_client_surface_metal_layer  →  يستقبل CAMetalLayer كـ "external" object
   (نفس نمط نقل الـ handles: __wine_..._handle)
2) dlls/winemac.drv/cocoa_window.m:
   - طبقة subclass من CAMetalLayer تعترض nextDrawable
   - قبل الاستدعاء: ترسل الطلب أعلاه إلى الخادم
   - الخادم يمرّرها إلى العملية المالكة للنافذة (owner) عبر unix call
3) dlls/winemac.drv/window.c:
   macdrv_client_surface_acquire_metal_swapchain():
     - بدل FIXME: macdrv_create_view_swapchain لطبقة مُستوردة من العملية الأخرى
       (عبر IOSurface مشترك، أو عبر CAMetalLayer مُسجَّل في الخادم)
```

**ملاحظة مهمّة (من RE):** CrossOver نفّذها على مسار **D3DMetal** (Apple) لا Vulkan:
هم يمرّرون `d3dmetal_client_surface` لأن D3DMetal يعمل **داخل العملية** ويطلب طبقة
Metal لنافذة child. لو أردنا نفس الشيء chez nous، فأقرب مسار مفتوح:
**vkd3d-proton + MoltenVK** — لكن MoltenVK يخلق الـ VkSurface في **عمليتنا**، فيصطدم
بحدود العملية ⇒ الحلMethode: **CAMetalLayer عبر IOSurface** (Apple permite
`CAMetalLayer` فوق IOSurface مشترك بين عمليتين).

**الجهد:** متوسط-كبير (٣ ملفات + بروتوكول خادم). **العائد:** كل نوافذ child تُعرض Metal ⇒
نهاية غلاف CEF نهائياً.

### الطبقة C —如果没有 cross-process path،路线 بديلة
- **ardo**: خلّي ستيم يعمل كعملية واحدة كاملة (steam + steamwebhelper مدمجين) ⇒ child
  في نفس العملية ⇒ مسار upstream ينجح بلا patch.
- **التسجيل**: لِـ `d3d11`/`dxgi` استعمل `wined3d` للواجهة فقط، وDXVK للعبة ⇒ فصل
  مسار العرض عن مسار اللعبة (كفاية متوسطة).

---

## 5) لماذا لا ننسخ تصحيح CodeWeavers حرفياً

`my_*` داخل `winemac.so` كودهم (ملكية تجارية)، ورخصة Apple/CodeWeavers على D3DMetal
لا تسمح بإعادة التوزيع. **نكتب التصحيح بنفس المنطق على سورس Wine (LGPL)** — ونضيفه
كـ PR حقيقي لـ Wine إن أراد أحد544投递ها، أو كـ patch على Gcenx عندنا.

---

## 6) خطة الاختبار (بعد أي طبقة)

| الحالة | كيف نتحقق |
|---|---|
| نافذة child تُعرض Metal | `WINEDEBUG=+macdrv` ⇒ لا `FIXME("Cross-process child window…")` + لا `VK_ERROR_INCOMPATIBLE_DRIVER` |
| لا شاشة سوداء | ستيم: «المكتبة» + «المتجر» + نافذة الإعدادات |
| الأداء | `DXVK_HUD=devinfo` + Metal capture + `sample` أثناء فتح المكتبة |
| عدم كسر Vulkan | لعبة DX11 عادية تشتغل بلا regressions + `vkcube` داخل القارورة |

---

## 7) ملخّص تنفيذي

- السبب **مُثبَت بالسورس**: `FIXME("Cross-process child window Metal swapchains are not implemented")`
  في `dlls/winemac.drv/window.c` (master سطر 1177) + `VK_ERROR_INCOMPATIBLE_DRIVER` في `vulkan.c`.
- **على Wine 11.0 ما فيه أصلاً Metal swapchain** ⇒ تأكد من إصدار محركك أولاً (11.12+).
- **الطبقة A (تغيير إعدادين)** قد تحلّ 80٪ من الحالة **اليوم** بلا أي fork.
- **الطبقة B (patch)** هي الحلّ الدائم، وهي قابلة للكتابة من الصفر على LGPL.
