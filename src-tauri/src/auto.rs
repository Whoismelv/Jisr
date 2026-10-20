// ============================================================
//  جسر — مخطط التشغيل التلقائي (Auto-Play Planner)
//  • منطق **خالص**: نأخذ «ما نعرفه عن اللعبة» + «ما هو مثبّت عندنا»
//    ونرجّع خطة ثابتة (deterministic): أي طبقة تُستعمل، أي تجاوز
//    ريجستري، أي تحذير، وأي مانع يمنع التشغيل.
//  • الهدف: زر «تشغيل» يشتغل من أول مرة — بدون تجارب يدوية ولا تخمين.
//  • هذا الملف ما ينفّذ ولا يكتب ولا يقرأ القرص: **قرار فقط**
//    (النفاذ الفعلي في graphics.rs / d3dx.rs / steam_library.rs).
//  • القاعدة الذهبية: ما نلمس ريجستري القارورة إلا لو الطلب واضح
//    (طبقة DXVK مثبتة +Override = native)، وإلا نكتفي بتحذير.
// ============================================================
use crate::steam_library::GameProbe;

// ============================================================
//  الأنواع
// ============================================================

/// الطبقة الرسومية التي ستُستعمل في هذه الخطة.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// DXVK ل DirectX 9 (d3d9.dll) — الطريق الأسرع على macOS
    Dxvk9,
    /// DXVK ل DirectX 10 (d3d10core.dll)
    Dxvk10,
    /// DXVK ل DirectX 11 (d3d11.dll)
    Dxvk11,
    /// D3DX9 الأصلي من مايكروسوفت (d3dx9_43 + d3dcompiler_43)
    D3dx9Native,
    /// WineD3D — مترجم GLSL المدمج في Wine (بطيء ومكسور على ماك)
    WineD3D,
    /// DXMT — DirectX 12 فوق Vulkan (يعمل على macOS عبر MoltenVK)
    DxmtD3d12,
    /// vkd3d — DirectX 12 فوق Vulkan (تغطية ضعيفة — تجريبي)
    Vkd3dD3d12,
    /// ما فيه طبقة: اللعبة ما تستخدم DirectX أصلاً (أو ما عرفنا)
    None,
}

impl Layer {
    /// المعرّف التقني (نفسه يُرسل للواجهة و JSON).
    pub fn as_str(self) -> &'static str {
        match self {
            Layer::Dxvk9 => "dxvk-d3d9",
            Layer::Dxvk10 => "dxvk-d3d10",
            Layer::Dxvk11 => "dxvk-d3d11",
            Layer::D3dx9Native => "d3dx9-native",
            Layer::WineD3D => "wined3d",
            Layer::DxmtD3d12 => "dxmt-d3d12",
            Layer::Vkd3dD3d12 => "vkd3d-d3d12",
            Layer::None => "none",
        }
    }

    /// اسم عربي قصير — للواجهة والسجل.
    pub fn label_ar(self) -> &'static str {
        match self {
            Layer::Dxvk9 => "DXVK (DirectX 9)",
            Layer::Dxvk10 => "DXVK (DirectX 10)",
            Layer::Dxvk11 => "DXVK (DirectX 11)",
            Layer::D3dx9Native => "D3DX9 الأصلي",
            Layer::WineD3D => "WineD3D (مترجم GLSL)",
            Layer::DxmtD3d12 => "DXMT (DirectX 12)",
            Layer::Vkd3dD3d12 => "vkd3d (DirectX 12)",
            Layer::None => "بلا طبقة",
        }
    }
}

impl std::fmt::Display for Layer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// إجراء تنفيذي مطلوب قبل التشغيل (الواجهة/الطبقات تترجمه لعملية).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// تثبيت طبقة (DXVK / D3DX9 / DXMT …)
    InstallLayer(Layer),
    /// كتابة تجاوز `native` لملف DLL في ريجستري القارورة
    EnableRegistryOverride(&'static str),
    /// إزالة تجاوز `native` (رجوع لنسخة Wine المدمجة)
    RemoveRegistryOverride(&'static str),
    /// ننصح المستخدم بتحديث محرك Wine (المشكلة من المحرك نفسه)
    RequestEngineUpdate,
    /// تحذير للمستخدم (ما يمنع التشغيل، لكن لازم يعرف)
    WarnUser(String),
    ///阻止 التشغيل — فيه سبب قاطع
    Block(String),
}

impl Action {
    /// نوع الإجراء (مفتاح ثابت للواجهة و JSON).
    pub fn kind(&self) -> &'static str {
        match self {
            Action::InstallLayer(_) => "install_layer",
            Action::EnableRegistryOverride(_) => "enable_registry_override",
            Action::RemoveRegistryOverride(_) => "remove_registry_override",
            Action::RequestEngineUpdate => "request_engine_update",
            Action::WarnUser(_) => "warn_user",
            Action::Block(_) => "block",
        }
    }

    /// نص مرافق الإجراء (الطبقة أو اسم الملف أو الرسالة).
    pub fn arg(&self) -> Option<String> {
        match self {
            Action::InstallLayer(l) => Some(l.as_str().to_string()),
            Action::EnableRegistryOverride(s) | Action::RemoveRegistryOverride(s) => {
                Some((*s).to_string())
            }
            Action::RequestEngineUpdate => None,
            Action::WarnUser(m) | Action::Block(m) => Some(m.clone()),
        }
    }
}

/// مانع يمنع (أو يهدّد) تشغيل اللعبة.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocker {
    /// مضاد غش يحتاج درايفر كيرنل ويندوز — مستحيل على macOS
    KernelAntiCheat,
    /// DirectX 12 — الطريق الوحيد المدعوم هو D3DMetal من Apple (ما نوزّعه)
    D3D12NeedsD3DMetal,
    /// DirectX 8 — ما فيه مسار (DXVK ما يدعمه، وWineD3D مكسور على ماك)
    Dx8Only,
    /// نسخة محمية بحماية ضد النسخ
    Drm,
    /// ما قدرنا نحدّد الواجهة المطلوبة
    Unknown,
}

impl Blocker {
    /// المعرّف التقني (للواجهة و JSON).
    pub fn as_str(self) -> &'static str {
        match self {
            Blocker::KernelAntiCheat => "kernel_anticheat",
            Blocker::D3D12NeedsD3DMetal => "d3d12_needs_d3dmetal",
            Blocker::Dx8Only => "dx8_only",
            Blocker::Drm => "drm",
            Blocker::Unknown => "unknown",
        }
    }

    /// هل يمنع التشغيل فعلاً؟ (`Unknown`conditionsinfo soft — نجهّز ونجرّب)
    pub fn is_hard(self) -> bool {
        !matches!(self, Blocker::Unknown)
    }

    /// وصف عربي قصير — يعرض كعنوان المانع.
    pub fn label_ar(self) -> &'static str {
        match self {
            Blocker::KernelAntiCheat => "مضاد غش يحتاج درايفر ويندوز",
            Blocker::D3D12NeedsD3DMetal => "DirectX 12 يحتاج D3DMetal (ما نقدر نوزّعه)",
            Blocker::Dx8Only => "DirectX 8 — ما فيه مسار على macOS",
            Blocker::Drm => "نسخة محمية بـ DRM",
            Blocker::Unknown => "ما قدرنا نحدّد DirectX المطلوب",
        }
    }
}

/// خطة التشغيل الكاملة — نرجّعها للواجهة وللتنفيذ.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayerPlan {
    /// الطبقات التي ستُستعمل (بعد كل القواعد)
    pub layers: Vec<Layer>,
    /// الإجراءات المطلوبة بالترتيب
    pub actions: Vec<Action>,
    /// الموانع المعروفة
    pub blockers: Vec<Blocker>,
    /// ملاحظات توضيحية (ليست موانع)
    pub notes: Vec<String>,
}

impl LayerPlan {
    /// هل اللعبة قابلة للتشغيل؟ (لا مانع قاطع)
    pub fn is_launchable(&self) -> bool {
        !self.blockers.iter().any(|b| b.is_hard())
    }

    /// هل الخطة فيها طبقة معيّنة؟
    pub fn has_layer(&self, layer: Layer) -> bool {
        self.layers.contains(&layer)
    }

    /// هل فيه إجراء من نوع معيّن؟
    pub fn has_action(&self, kind: &str) -> bool {
        self.actions.iter().any(|a| a.kind() == kind)
    }
}

/// ما هو متاح عندنا فعلاً (يُقرأ من graphics.rs / d3dx.rs / المحرك).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerAvailability {
    /// DXVK مثبّت (d3d10core + d3d11 للمعمارين)
    pub dxvk: bool,
    /// DXVK فيه d3d9.dll (النسخة الخاصة بألعاب DirectX 9)
    pub dxvk_d3d9: bool,
    /// D3DX9 الأصلي (d3dx9_43 + d3dcompiler_43) مثبّت
    pub d3dx9: bool,
    /// DXMT (DirectX 12 فوق Vulkan) مثبّت
    pub dxmt: bool,
    /// vkd3d (DirectX 12 فوق Vulkan) متاح
    pub vkd3d: bool,
    /// محرك Wine فيه vulkan-1.dll للـ32-بت (بدونه DXVK 32-بت ما يحمّل)
    pub engine_has_i386_vulkan: bool,
}

/// كل ما نعرفه عن اللعبة (فحص PE + الحاجات الخاصة بها).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameFacts {
    /// نتيجة فحص الملف التنفيذي
    pub probe: GameProbe,
    /// اللعبة تستدعي d3dx9_43.dll (ألعاب DX9 القديمة)
    pub needs_d3dx9: bool,
    /// اللعبة تستدعي d3dcompiler_43.dll (تجميع shaders وقت التشغيل)
    pub needs_d3dcompiler: bool,
}

// ============================================================
//  الرسائل (عربية — الواجهة تتركّب منها نصوصها)
// ============================================================

const ANTI_CHEAT_BLOCK: &str =
    "مضاد الغش في هذه اللعبة يحتاج درايفر على مستوى الكيرنل في ويندوز (مثل Easy Anti-Cheat أو BattlEye). \
macOS ما يقدر يحمّل درايفر ويندوز، فالتشغيل مستحيل — ما راح نحاول نخدعك بتجاوز.";
const ANTI_CHEAT_NOTE: &str =
    "مضاد الغش (kernel anti-cheat) مستحيل على macOS:Wine يشغّل المستخدم العادي، لكن الدرايفر ما له بديل.";

const D3D12_BLOCK: &str =
    "DirectX 12 يحتاج D3DMetal من Apple — ما نقدر نوزّعه (ترخيص Apple). \
بدون DXMT ما راح تشتغل اللعبة على macOS.";
const D3D12_NOTE: &str =
    "Apple ما تسمح بتوزيع D3DMetal، والبديل الوحيد على macOS هو DXMT (DirectX 12 فوق Vulkan → MoltenVK → Metal).";
const D3D12_VKD3D_NOTE: &str =
    "vkd3d متاح كبديل تجريبي لـDirectX 12، لكن تغطيته ضعيفة — أغلب ألعاب D3D12 ما تشتغل به.";

const DXVK_MISSING_WARN: &str =
    "طبقة DXVK مو مثبّتة — Wine يستخدم WineD3D (ترجمة GLSL)، وعلى macOS هذا يعني غالباً \
«Pixel shader error … E5000: syntax error» وشاشة سودا. ثبّت طبقة DXVK من صفحة الرسوميات.";
const DXVK9_MISSING_WARN: &str =
    "نسخة DXVK المثبّتة ما فيها d3d9.dll ⇒ Wine يسقط على WineD3D (ترجمة GLSL) ⇒ مُصرّف GLSL من Apple \
يرفض الـshader («Pixel shader error … E5000») ⇒ شاشة سودا. نزّل نسخة DXVK اللي فيها d3d9.dll.";
const DXVK9_OK_NOTE: &str =
    "DXVK يحوّل DirectX 9 إلى Vulkan → Metal: أسرع وأدق من WineD3D على ألعاب DX9.";
const DXVK11_OK_NOTE: &str =
    "DXVK يحوّل DirectX 10/11 إلى Vulkan → MoltenVK → Metal — نفس مسار Whisky.";
const WINE_D3D_FALLBACK_NOTE: &str =
    "WineD3D يترجم GLSL علىflies:Slowest path، وعلى ماك مُصرّف GLSL من Apple يرفض أغلب الـshaders.";

const D3DX9_MISSING_NOTE: &str =
    "اللعبة تحتاج d3dx9_43.dll — النسخة المدمجة في Wine تنهار (page fault) في ألعاب DX9 القديمة. \
ثبّت D3DX9 الأصلي (حزمة DirectX End-User Runtime) — نفس اللي CrossOver وwinetricks يسوونه.";
const D3DX9_OK_NOTE: &str = "D3DX9 الأصلي جاهز — ألعاب DX9 القديمة ما تنهار على d3dx9_43.";

const I386_VULKAN_WARN: &str =
    "اللعبة 32-بت، ومحرّك Wine ما فيه vulkan-1.dll للـi386 ⇒ DXVK ما يقدر يحمّل ⇒ Wine يرجع لـWineD3D \
(ترجمة GLSL) ⇒ نفس خطأ الـpixel shader (شاشة سودا). حدّث المحرك لو تبي دعم 32-بت.";
const I386_VULKAN_NOTE: &str =
    "دعم 32-بت يحتاج vulkan-1.dll في i386-windows داخل محرّك Wine — بدونه DXVK ما يشتغل للألعاب القديمة.";

const DX8_BLOCK: &str =
    "DirectX 8 ما له مسار على macOS: DXVK ما يدعم d3d8، وWineD3D يكسر عند ترجمة GLSL في Apple. \
ما فيه حل مضمون — تشغيل اللعبة على ويندوز/لينكس هو الخيار الوحيد.";
const DX8_NOTE: &str = "بدائل DirectX 8:.port/ dxvk-d3d8 غير رسمي، أو older engine — الكود قديم جداً.";

const D3D12_I386_NOTE: &str =
    "DXMT يدعم 64-بت فقط — لعبة 32-بت تحتاج DirectX 12 ما راح تشتغل بطبقة DXMT.";

const UNKNOWN_WARN: &str =
    "ما قدرنا نحدّد واجهة الرسوم في ملف اللعبة (ما فيها import معروف لـDirectX). \
شغّل اللعبة مرة واحدة وشاركنا آخر 20 سطر من السجل — بعدها نضبط الطبقة المناسبة deterministically.";
const UNKNOWN_NOTE: &str =
    "ألعاب كثير تستدعي d3d11 عبر LoadLibrary وقت التشغيل، فالفحص الساكن ما يشوفها — لازم سجل التشغيل.";
const ENGINE_UPDATE_NOTE: &str =
    "ما قدرنا نقرأ ملف اللعبة — جرّب تحدّث محرّك Wine (نسخة أحدث تقرأ PE أفضل وتكشف DirectX 12 و32-بت).";

const WINED3D_MODE_NOTE: &str =
    "وضع الرسوميات الحالي WineD3D (اختيارك الصريح) — عطّلنا طبقات DXVK وأزلنا تجاوزات native.";

const DRM_BLOCK: &str =
    "نسخة اللعبة محمية بحماية ضد النسخ (DRM) — ما نقدر نشغّلها.";
const DRM_NOTE: &str = "حماية DRM تعتمد على خدمات ويندوز (أو سكربتات) — ما عندنا بديل على macOS.";

const VULKAN_NOTE: &str =
    "اللعبة تستدعي Vulkan-1.dll أصلياً ⇒ هي ليست DirectX أصلاً، ومحرك Wine عندنا ما يوفّر Vulkan 1.3 كامل. \
جرّب تشغيلها أولاً — Wine 9+ مع DXVK ممكن يمشي.";
const OPENGL_NOTE: &str =
    "اللعبة تستخدم OpenGL — Wine يترجمها إلى OpenGL/Metal، وغالباً تشتغل (أو تتعطّل بسبب GLSL). \
فعّل وضع النافذة من إعدادات اللعبة أول مرة.";

/// علامات مضاد الغش في اسم الملف التنفيذي الرئيسي.
/// (نفس قائمة `uses_kernel_anticheat` في steam_library.rs)
const ANTI_CHEAT_MARKERS: [&str; 7] = [
    "easyanticheat",
    "battleye",
    "beclient",
    "vanguard",
    "gameguard",
    "xignode",
    "xigncode",
];

/// علامات حماية DRM في اسم الملف التنفيذي (نادر — نتحقق بحذر).
const DRM_MARKERS: [&str; 4] = ["denuvo", "safedisc", "securom", "starforce"];

// ============================================================
//  أدوات فحص صغيرة (بلا I/O — كل شي من البيانات الجاهزة)
// ============================================================

/// اسم الملف التنفيذي (بدون المسار) — تتجاهل الحروف الكبيرة.
fn exe_file_name(probe: &GameProbe) -> String {
    let raw = probe.exe.as_deref().unwrap_or_default();
    let name = raw
        .rsplit(['\\', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or_default();
    name.to_ascii_lowercase()
}

/// هل اللعبة تستخدم مضاد غش يحتاج درايفر كيرنل؟
fn has_kernel_anticheat(probe: &GameProbe) -> bool {
    if probe
        .issues
        .iter()
        .any(|i| i.to_ascii_lowercase().contains("anticheat"))
    {
        return true;
    }
    let name = exe_file_name(probe);
    if name.is_empty() {
        return false;
    }
    ANTI_CHEAT_MARKERS.iter().any(|m| name.contains(m))
        // "vgc" قصيرة جداً — نقبلها بس في بداية الاسم (وحدة تحكّم Riot)
        || name.starts_with("vgc")
}

/// هل اسم الملف التنفيذي يكشف حماية DRM؟
fn has_drm_marker(probe: &GameProbe) -> bool {
    let name = exe_file_name(probe);
    if name.is_empty() {
        return false;
    }
    DRM_MARKERS.iter().any(|m| name.contains(m))
}

/// اللعبة 32-بت؟ (نثق بعلامة "x86" لو المعمارية ما انقرأت)
fn is_x86(probe: &GameProbe) -> bool {
    match probe.is_64 {
        Some(v) => !v,
        None => probe.issues.iter().any(|i| i.eq_ignore_ascii_case("x86")),
    }
}

/// نضيف طبقة بدون تكرار.
fn push_layer(plan: &mut LayerPlan, layer: Layer) {
    if !plan.layers.contains(&layer) {
        plan.layers.push(layer);
    }
}

// ============================================================
//  القواعد — كل واحدة في دالة مستقلة (اختبار أسهل،责任 واضحة)
// ============================================================

/// DirectX 12 — DXMT إن توفّر، وإلا مانع D3DMetal (+ vkd3d تجريبي).
fn plan_d3d12(plan: &mut LayerPlan, avail: &LayerAvailability, x86: bool) {
    if avail.dxmt {
        push_layer(plan, Layer::DxmtD3d12);
        plan.actions.push(Action::InstallLayer(Layer::DxmtD3d12));
        plan.actions.push(Action::EnableRegistryOverride("d3d12"));
        plan.notes.push(D3D12_NOTE.to_string());
        if x86 {
            plan.notes.push(D3D12_I386_NOTE.to_string());
        }
        return;
    }

    // DXMT مو متاح ⇒ مانع: Apple ما تسمح بتوزيع D3DMetal
    plan.blockers.push(Blocker::D3D12NeedsD3DMetal);
    plan.actions.push(Action::Block(D3D12_BLOCK.to_string()));
    plan.notes.push(D3D12_NOTE.to_string());
    if avail.vkd3d {
        push_layer(plan, Layer::Vkd3dD3d12);
        plan.actions.push(Action::InstallLayer(Layer::Vkd3dD3d12));
        plan.actions.push(Action::EnableRegistryOverride("d3d12"));
        plan.notes.push(D3D12_VKD3D_NOTE.to_string());
    }
}

/// DirectX 11 / 10 — DXVK (d3d11 / d3d10core) أو WineD3D.
fn plan_dx10_11(
    plan: &mut LayerPlan,
    avail: &LayerAvailability,
    x86: bool,
    layer: Layer,
    override_name: &'static str,
) {
    // DXVK 32-بت يحتاج vulkan-1.dll في المحرك
    let can_dxvk = avail.dxvk && (!x86 || avail.engine_has_i386_vulkan);
    if can_dxvk {
        push_layer(plan, layer);
        plan.actions.push(Action::EnableRegistryOverride(override_name));
        plan.notes.push(DXVK11_OK_NOTE.to_string());
        return;
    }
    push_layer(plan, Layer::WineD3D);
    if x86 {
        plan.actions.push(Action::WarnUser(I386_VULKAN_WARN.to_string()));
    } else {
        plan.actions.push(Action::WarnUser(DXVK_MISSING_WARN.to_string()));
    }
    plan.notes.push(WINE_D3D_FALLBACK_NOTE.to_string());
}

/// DirectX 9 — DXVK-d3d9 إن توفّر، وإلا WineD3D (screen black متوقّع).
fn plan_d3d9(plan: &mut LayerPlan, avail: &LayerAvailability) {
    if avail.dxvk_d3d9 {
        push_layer(plan, Layer::Dxvk9);
        if !avail.dxvk {
            // حالة نادرة: d3d9 موجود بس الحزمة ناقصة — نطلب التثبيت
            plan.actions.push(Action::InstallLayer(Layer::Dxvk9));
        }
        plan.actions.push(Action::EnableRegistryOverride("d3d9"));
        plan.notes.push(DXVK9_OK_NOTE.to_string());
        return;
    }
    push_layer(plan, Layer::WineD3D);
    plan.actions.push(Action::WarnUser(DXVK9_MISSING_WARN.to_string()));
    plan.notes.push(WINE_D3D_FALLBACK_NOTE.to_string());
}

/// DirectX 8 — ما فيه مسار.
fn plan_d3d8(plan: &mut LayerPlan) {
    plan.blockers.push(Blocker::Dx8Only);
    plan.actions.push(Action::Block(DX8_BLOCK.to_string()));
    push_layer(plan, Layer::WineD3D);
    plan.notes.push(DX8_NOTE.to_string());
}

/// Vulkan أصلي — Wine عندنا ما يوفّر Vulkan 1.3 كامل.
fn plan_vulkan(plan: &mut LayerPlan) {
    push_layer(plan, Layer::None);
    plan.actions.push(Action::WarnUser(VULKAN_NOTE.to_string()));
    plan.notes.push(VULKAN_NOTE.to_string());
}

/// OpenGL —Wine يترجمه، وغالباً يشتغل.
fn plan_opengl(plan: &mut LayerPlan) {
    push_layer(plan, Layer::None);
    plan.actions.push(Action::WarnUser(OPENGL_NOTE.to_string()));
    plan.notes.push(OPENGL_NOTE.to_string());
}

/// ما عرفنا الواجهة — نطلب سجل التشغيل.
fn plan_unknown(plan: &mut LayerPlan, probe: &GameProbe) {
    push_layer(plan, Layer::None);
    plan.actions.push(Action::WarnUser(UNKNOWN_WARN.to_string()));
    plan.notes.push(UNKNOWN_NOTE.to_string());
    if probe.exe.is_none() {
        // ما قدرنا نقرأ ملف اللعبة أصلاً ⇒ نطلب سجل + تحديث المحرك
        plan.blockers.push(Blocker::Unknown);
        plan.actions.push(Action::RequestEngineUpdate);
        plan.notes.push(ENGINE_UPDATE_NOTE.to_string());
    }
}

/// D3DX9 الأصلي — مطلوب لألعاب DX9 القديمة (d3dx9_43 + d3dcompiler_43).
fn plan_d3dx9(plan: &mut LayerPlan, facts: &GameFacts, avail: &LayerAvailability) {
    if !(facts.needs_d3dx9 || facts.needs_d3dcompiler) {
        return;
    }
    push_layer(plan, Layer::D3dx9Native);
    if avail.d3dx9 {
        plan.notes.push(D3DX9_OK_NOTE.to_string());
        return;
    }
    plan.actions.push(Action::InstallLayer(Layer::D3dx9Native));
    plan.notes.push(D3DX9_MISSING_NOTE.to_string());
}

/// وضع WineD3D الصريح — نحترم اختيار المستخدم: طبقة واحدة + إزالة التجاوزات.
fn apply_wined3d_mode(plan: &mut LayerPlan) {
    // الطبقات: WineD3D فقط (D3DX9 الأصلي يبقى كإجراء — مستقل عن مسار الرسوم)
    let mut layers: Vec<Layer> = Vec::new();
    for l in plan.layers.iter() {
        if !layers.contains(l) {
            layers.push(*l);
        }
    }
    let d3dx_needed = plan.layers.contains(&Layer::D3dx9Native);
    plan.layers = vec![Layer::WineD3D];
    if d3dx_needed {
        plan.notes
            .push("D3DX9 الأصلي مطلوب حتى مع WineD3D (d3dx9_43 مستقل عن مسار الرسوم).".to_string());
    }

    // التجاوزات: native ⇒ إزالتها (رجوع لنسخة Wine المدمجة)
    let mut actions: Vec<Action> = Vec::new();
    for a in plan.actions.drain(..) {
        match a {
            Action::EnableRegistryOverride(name) => {
                actions.push(Action::RemoveRegistryOverride(name));
            }
            Action::InstallLayer(l) if l != Layer::D3dx9Native => {
                // ما ننصّب طبقات DXVK في وضع WineD3D
            }
            other => actions.push(other),
        }
    }
    plan.actions = actions;
    plan.notes.push(WINED3D_MODE_NOTE.to_string());
}

// ============================================================
//  الدالة الأساسية
// ============================================================

/// الخطة مع كل الحقائق (بما فيها d3dx9/d3dcompiler).
pub fn plan_with_facts(
    facts: &GameFacts,
    avail: &LayerAvailability,
    mode: &str,
) -> LayerPlan {
    let probe = &facts.probe;
    let dx = probe
        .dx
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let x86 = is_x86(probe);
    let mut plan = LayerPlan::default();

    // 1) مضاد الغش — أولوية قصوى (ما فيه طبقة تنقذ درايفر كيرنل)
    if has_kernel_anticheat(probe) {
        plan.blockers.push(Blocker::KernelAntiCheat);
        plan.actions.push(Action::Block(ANTI_CHEAT_BLOCK.to_string()));
        plan.notes.push(ANTI_CHEAT_NOTE.to_string());
    }

    // 2) DRM
    if has_drm_marker(probe) {
        plan.blockers.push(Blocker::Drm);
        plan.actions.push(Action::Block(DRM_BLOCK.to_string()));
        plan.notes.push(DRM_NOTE.to_string());
    }

    // 3) الطبقة حسب مستوى DirectX
    match dx.as_str() {
        "d3d12" => plan_d3d12(&mut plan, avail, x86),
        "d3d11" => plan_dx10_11(&mut plan, avail, x86, Layer::Dxvk11, "d3d11"),
        "d3d10" => plan_dx10_11(&mut plan, avail, x86, Layer::Dxvk10, "d3d10core"),
        "d3d9" => plan_d3d9(&mut plan, avail),
        "d3d8" => plan_d3d8(&mut plan),
        "vulkan" => plan_vulkan(&mut plan),
        "opengl" => plan_opengl(&mut plan),
        _ => plan_unknown(&mut plan, probe),
    }

    // 4) D3DX9 الأصلي (ألعاب DX9 القديمة)
    plan_d3dx9(&mut plan, facts, avail);

    // 5) 32-بت بدون Vulkan للـi386 ⇒ DXVK 32-بت ما يحمّل
    if x86 && avail.dxvk_d3d9 && !avail.engine_has_i386_vulkan {
        plan.actions.push(Action::WarnUser(I386_VULKAN_WARN.to_string()));
        plan.notes.push(I386_VULKAN_NOTE.to_string());
    }

    // 6) اختيار المستخدم: WineD3D ⇒ طبقة واحدة + إزالة كل التجاوزات
    if mode.trim().eq_ignore_ascii_case("wined3d") {
        apply_wined3d_mode(&mut plan);
    }

    plan
}

/// الخطة الأساسية: نأخذ فحص اللعبة + المتاح + وضع الرسوميات.
pub fn plan(probe: &GameProbe, avail: &LayerAvailability, mode: &str) -> LayerPlan {
    let facts = GameFacts {
        probe: probe.clone(),
        needs_d3dx9: false,
        needs_d3dcompiler: false,
    };
    plan_with_facts(&facts, avail, mode)
}

// ============================================================
//  التحويل للنص و JSON
// ============================================================

/// سطر واحد يشرح الإجراء بالعربي.
fn action_text(action: &Action) -> String {
    match action {
        Action::InstallLayer(l) => format!("تثبيت طبقة {}", l.label_ar()),
        Action::EnableRegistryOverride(name) => {
            format!("تفعيل تجاوز native لملف {name} في ريجستري القارورة")
        }
        Action::RemoveRegistryOverride(name) => {
            format!("إلغاء تجاوز {name} (رجوع لنسخة Wine المدمجة)")
        }
        Action::RequestEngineUpdate => "ننصح بتحديث محرّك Wine".to_string(),
        Action::WarnUser(msg) => format!("تحذير: {msg}"),
        Action::Block(msg) => format!("منع التشغيل: {msg}"),
    }
}

/// ملخص عربي قصير — للواجهة و سجل التشغيل (سطر لكل عنصر).
pub fn plan_to_text(plan: &LayerPlan) -> String {
    let mut out = String::new();
    out.push_str("خطة التشغيل التلقائي:\n");
    if plan.layers.is_empty() {
        out.push_str("• الطبقات: لا شيء\n");
    } else {
        for l in plan.layers.iter() {
            out.push_str(&format!("• طبقة: {}\n", l.label_ar()));
        }
    }
    for b in plan.blockers.iter() {
        out.push_str(&format!("• مانع: {}\n", b.label_ar()));
    }
    for a in plan.actions.iter() {
        out.push_str(&format!("• {}\n", action_text(a)));
    }
    for n in plan.notes.iter() {
        out.push_str(&format!("• ملاحظة: {n}\n"));
    }
    out
}

/// الإجراء بصيغة JSON: `{kind, arg}`.
fn action_json(action: &Action) -> serde_json::Value {
    let kind = action.kind();
    match action.arg() {
        Some(arg) => serde_json::json!({ "kind": kind, "arg": arg }),
        None => serde_json::json!({ "kind": kind, "arg": serde_json::Value::Null }),
    }
}

/// الخطة بصيغة JSON — نرسلها للواجهة.
pub fn plan_to_json(plan: &LayerPlan) -> serde_json::Value {
    let layers: Vec<&str> = plan.layers.iter().map(|l| l.as_str()).collect();
    let actions: Vec<serde_json::Value> = plan.actions.iter().map(action_json).collect();
    let blockers: Vec<&str> = plan.blockers.iter().map(|b| b.as_str()).collect();
    serde_json::json!({
        "layers": layers,
        "actions": actions,
        "blockers": blockers,
        "notes": plan.notes,
        "launchable": plan.is_launchable(),
    })
}

// ============================================================
//  الاختبارات
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    /// فحص لعبة كامل (افتراضي: 64-بت + d3d11).
    fn probe(dx: Option<&str>, is_64: Option<bool>) -> GameProbe {
        GameProbe {
            exe: Some("C:\\Games\\demo\\demo.exe".to_string()),
            dx: dx.map(|s| s.to_string()),
            is_64,
            issues: Vec::new(),
        }
    }

    /// كل الطبقات متاحة.
    fn avail_all() -> LayerAvailability {
        LayerAvailability {
            dxvk: true,
            dxvk_d3d9: true,
            d3dx9: true,
            dxmt: true,
            vkd3d: false,
            engine_has_i386_vulkan: true,
        }
    }

    #[test]
    fn dx9_with_dxvk_uses_dxvk_and_native_override() {
        let p = plan(&probe(Some("d3d9"), Some(true)), &avail_all(), "auto");
        assert!(p.has_layer(Layer::Dxvk9));
        assert!(!p.has_layer(Layer::WineD3D));
        assert!(p
            .actions
            .contains(&Action::EnableRegistryOverride("d3d9")));
        assert!(p.blockers.is_empty());
        assert!(p.is_launchable());
        // ما في تحذير GLSL — DXVK موجود
        assert!(!p.has_action("warn_user"));
    }

    #[test]
    fn dx9_without_dxvk_d3d9_falls_back_to_wined3d_with_glsl_warning() {
        let mut a = avail_all();
        a.dxvk_d3d9 = false;
        let p = plan(&probe(Some("d3d9"), Some(true)), &a, "auto");
        assert!(p.has_layer(Layer::WineD3D));
        assert!(!p.has_layer(Layer::Dxvk9));
        // ما فيه تجاوز native — WineD3D هو المدمج
        assert!(!p
            .actions
            .iter()
            .any(|x| matches!(x, Action::EnableRegistryOverride(_))));
        let text = plan_to_text(&p);
        assert!(text.contains("GLSL"), "لازم نذكر فشل الـshader: {text}");
        assert!(text.contains("E5000"), "لازم نذكر كود الخطأ: {text}");
    }

    #[test]
    fn dx9_needing_d3dx9_gets_install_action_when_missing() {
        let mut a = avail_all();
        a.d3dx9 = false;
        let facts = GameFacts {
            probe: probe(Some("d3d9"), Some(true)),
            needs_d3dx9: true,
            needs_d3dcompiler: true,
        };
        let p = plan_with_facts(&facts, &a, "auto");
        assert!(p.actions.contains(&Action::InstallLayer(Layer::D3dx9Native)));
        assert!(p.has_layer(Layer::D3dx9Native));
        let text = plan_to_text(&p);
        assert!(text.contains("d3dx9_43"), "لازم نذكر d3dx9_43: {text}");
        // d3d9 نفسه لسه DXVK
        assert!(p.has_layer(Layer::Dxvk9));
    }

    #[test]
    fn d3dx9_already_installed_needs_no_install_action() {
        let facts = GameFacts {
            probe: probe(Some("d3d9"), Some(true)),
            needs_d3dx9: true,
            needs_d3dcompiler: false,
        };
        let p = plan_with_facts(&facts, &avail_all(), "auto");
        assert!(!p
            .actions
            .contains(&Action::InstallLayer(Layer::D3dx9Native)));
        assert!(p.has_layer(Layer::D3dx9Native));
    }

    #[test]
    fn dx12_with_dxmt_uses_dxmt_and_d3d12_override() {
        let p = plan(&probe(Some("d3d12"), Some(true)), &avail_all(), "auto");
        assert!(p.has_layer(Layer::DxmtD3d12));
        assert!(p.actions.contains(&Action::InstallLayer(Layer::DxmtD3d12)));
        assert!(p
            .actions
            .contains(&Action::EnableRegistryOverride("d3d12")));
        assert!(p.blockers.is_empty());
        assert!(p.is_launchable());
    }

    #[test]
    fn dx12_without_dxmt_is_blocked_with_d3dmetal_explanation() {
        let mut a = avail_all();
        a.dxmt = false;
        a.vkd3d = false;
        let p = plan(&probe(Some("d3d12"), Some(true)), &a, "auto");
        assert!(p.blockers.contains(&Blocker::D3D12NeedsD3DMetal));
        assert!(!p.has_layer(Layer::DxmtD3d12));
        assert!(!p.is_launchable());
        let text = plan_to_text(&p);
        assert!(text.contains("D3DMetal"), "لازم نذكر D3DMetal: {text}");
    }

    #[test]
    fn kernel_anticheat_blocks_but_still_lists_layers() {
        let mut pr = probe(Some("d3d11"), Some(true));
        pr.issues.push("anticheat".to_string());
        let p = plan(&pr, &avail_all(), "auto");
        assert!(p.blockers.contains(&Blocker::KernelAntiCheat));
        assert!(p.has_action("block"));
        assert!(!p.is_launchable());
        // الخطة لسه تبيّن الطبقة (التشخيص) — بس مو قابلة للتشغيل
        assert!(p.has_layer(Layer::Dxvk11));
    }

    #[test]
    fn anticheat_detected_from_exe_name_too() {
        let mut pr = probe(Some("d3d9"), Some(true));
        pr.exe = Some("C:\\Games\\demo\\EasyAntiCheat_EOS.exe".to_string());
        let p = plan(&pr, &avail_all(), "auto");
        assert!(p.blockers.contains(&Blocker::KernelAntiCheat));
    }

    #[test]
    fn wined3d_mode_forces_wined3d_and_removes_overrides() {
        let p = plan(&probe(Some("d3d11"), Some(true)), &avail_all(), "wined3d");
        assert_eq!(p.layers, vec![Layer::WineD3D]);
        assert!(!p
            .actions
            .iter()
            .any(|a| matches!(a, Action::EnableRegistryOverride(_))));
        assert!(p
            .actions
            .contains(&Action::RemoveRegistryOverride("d3d11")));
        // ما ننصّب DXVK في وضع WineD3D
        assert!(!p
            .actions
            .iter()
            .any(|a| matches!(a, Action::InstallLayer(Layer::Dxvk11))));
    }

    #[test]
    fn x86_without_i386_vulkan_warns_about_32bit_dxvk() {
        let mut a = avail_all();
        a.engine_has_i386_vulkan = false;
        let p = plan(&probe(Some("d3d9"), Some(false)), &a, "auto");
        // تحذير (مو مانع قاطع) — اللعبة ممكن تشتغل لو DXVK ما احتاج Vulkan
        assert!(p.has_action("warn_user"));
        assert!(p.blockers.is_empty());
        let text = plan_to_text(&p);
        assert!(text.contains("32"), "لازم نذكر 32-بت: {text}");
    }

    #[test]
    fn x86_with_i386_vulkan_has_no_i386_warning() {
        let p = plan(&probe(Some("d3d9"), Some(false)), &avail_all(), "auto");
        assert!(!p.has_action("warn_user"));
    }

    #[test]
    fn unknown_dx_asks_for_log_and_requests_engine_update() {
        let pr = GameProbe {
            exe: None,
            dx: None,
            is_64: None,
            issues: Vec::new(),
        };
        let p = plan(&pr, &avail_all(), "auto");
        assert!(p.has_layer(Layer::None));
        assert!(p.has_action("warn_user"));
        assert!(p.has_action("request_engine_update"));
        assert!(p.blockers.contains(&Blocker::Unknown));
        // مانع غير قاطع: ما نمنع المستخدم من المحاولة
        assert!(p.is_launchable());
    }

    #[test]
    fn dx11_64bit_uses_dxvk11_and_d3d11_override() {
        let p = plan(&probe(Some("d3d11"), Some(true)), &avail_all(), "auto");
        assert!(p.has_layer(Layer::Dxvk11));
        assert!(p.actions.contains(&Action::EnableRegistryOverride("d3d11")));
        assert!(p.blockers.is_empty());
    }

    #[test]
    fn dx10_64bit_uses_dxvk10_and_d3d10core_override() {
        let p = plan(&probe(Some("d3d10"), Some(true)), &avail_all(), "auto");
        assert!(p.has_layer(Layer::Dxvk10));
        assert!(p
            .actions
            .contains(&Action::EnableRegistryOverride("d3d10core")));
    }

    #[test]
    fn dx8_is_a_hard_blocker() {
        let p = plan(&probe(Some("d3d8"), Some(true)), &avail_all(), "auto");
        assert!(p.blockers.contains(&Blocker::Dx8Only));
        assert!(!p.is_launchable());
    }

    #[test]
    fn json_shape_is_stable() {
        let p = plan(&probe(Some("d3d9"), Some(true)), &avail_all(), "auto");
        let j = plan_to_json(&p);
        let layers = j["layers"].as_array().expect("طبقات مصفوفة");
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].as_str(), Some("dxvk-d3d9"));
        let actions = j["actions"].as_array().expect("إجراءات مصفوفة");
        assert!(!actions.is_empty());
        assert!(actions
            .iter()
            .all(|a| a["kind"].is_string() && a.get("arg").is_some()));
        assert!(j["blockers"].as_array().expect("موانع مصفوفة").is_empty());
        assert!(j["notes"].as_array().expect("ملاحظات مصفوفة").is_empty());
        assert_eq!(j["launchable"].as_bool(), Some(true));
    }

    #[test]
    fn text_summary_lists_layers_and_actions() {
        let p = plan(&probe(Some("d3d9"), Some(true)), &avail_all(), "auto");
        let text = plan_to_text(&p);
        assert!(text.contains("DXVK"));
        assert!(text.contains("d3d9"));
        assert!(text.lines().count() >= 3);
    }

    #[test]
    fn empty_probe_does_not_panic() {
        let pr = GameProbe::default();
        let p = plan(&pr, &LayerAvailability::default(), "auto");
        assert!(p.has_layer(Layer::None));
        assert!(p.has_action("warn_user"));
    }

    #[test]
    fn unknown_mode_behaves_like_auto() {
        let p = plan(&probe(Some("d3d9"), Some(true)), &avail_all(), "نص عشوائي");
        assert!(p.has_layer(Layer::Dxvk9));
    }
}
