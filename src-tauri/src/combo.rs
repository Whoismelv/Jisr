// ============================================================
//  جسر — الملف الشخصي للتوافق لكل لعبة (Compatibility Profile)
//  • كل لعبة لها «تركيبة» خاصة: DXVK/WineD3D + D3DX9 + المترجم +
//    محرك الرسم (vulkan/gl) + نافذة مفروضة + Steam overlay
//  • عند الانهيار: تصنيفه (triage) يعطي رمز إجراء، وهنا نحوّله إلى
//    «التركيبة التالية» → إعادة تشغيل تلقائية بلا تدخل المستخدم
//  • التركيبة الناجحة تُحفظ لكل لعبة (ملف لكل appid) في
//    <data_dir>/profiles/<appid>.json
// ============================================================
//! # الملف الشخصي لكل لعبة — ليش هذا الملف موجود؟
//!
//! **المشكلة الميدانية:** إعداد واحد عالمي ما يناسب كل الألعاب. لعبة DX9
//! قد تعمل مع DXVK + D3DX9 الأصلي، وأخرى تنهار إلا مع WineD3D، وأخرى
//! ما تنهار إلا بعد تعطيل الـoverlay (هوكل على `IDirect3DDevice9::CreateDevice`).
//! اليوم نطبّق إعداداً واحداً للجميع ⇒ إما لعبة ما تشتغل، إما لاعب
//! يضيع وقتاً في تجارب يدوية.
//!
//! **الحل:** نحفظ لكل لعبة *التركيبة التي نجحت معها*، وعند الانهار
//! نصنّفه وننتقل إلى *التركيبة التالية* بدل ما نخسر وقت المستخدم.
//!
//! **حدود الملف:** منطق خالص + تخزين JSON بسيط. **ما ينفّذ** ولا يكتب في
//! الريجستري ولا يقلع Wine — التنفيذ في `graphics.rs` و`steam_library.rs`.
//!
//! **حدود الأمان:** أي ملف تالف يرجع `None` بدل ما ينهار التطبيق، والكتابة
//! ذرّية (ملف مؤقت + إعادة تسمية) عشان ما يبقى ملف نص/نص يفسّر اللعبة.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// اسم مجلد الملفات الشخصية داخل مجلد بيانات جسر.
pub const PROFILE_DIR_NAME: &str = "profiles";

/// صيغة الملف على القرص — نرفعها لو تغيّر الشكل مستقبلاً (الملفات القديمة
/// نتجاهلها بدل ما نكسرها).
const PROFILE_VERSION: u64 = 1;

// ============================================================
//  الأنواع
// ============================================================

/// طبقة DirectX 9 التي تشتغل بها اللعبة (d3d9.dll).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D3d9Layer {
    /// DXVK — أسرع وأدق على macOS (يترجم DirectX إلى Vulkan فوق MoltenVK).
    Dxvk,
    /// WineD3D — مترجم GLSL المدمج في Wine (بطيء، ومكسور مع شيدرات حديثة).
    WineD3D,
}

impl D3d9Layer {
    /// المعرّف التقني (نفسه في JSON والواجهة).
    pub fn as_str(self) -> &'static str {
        match self {
            D3d9Layer::Dxvk => "dxvk",
            D3d9Layer::WineD3D => "wined3d",
        }
    }

    /// قراءة من نص JSON — نقبل بعض المرادفات القديمة (الملفات تتطوّر).
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dxvk" => Some(D3d9Layer::Dxvk),
            "wined3d" | "wine-d3d" | "wine_d3d" | "builtin" => Some(D3d9Layer::WineD3D),
            _ => None,
        }
    }
}

/// طبقة D3DX9 (d3dx9_43.dll) — النسخة الأصلية من مايكروسوفت أو المدمجة في Wine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum D3dx9Layer {
    /// الأصلية من حزمة DirectX June 2010 (تنزّلها `d3dx.rs`).
    Native,
    /// المدمجة في Wine (تنهار في ألعاب DX9 القديمة على macOS).
    Builtin,
}

impl D3dx9Layer {
    /// المعرّف التقني (نفسه في JSON والواجهة).
    pub fn as_str(self) -> &'static str {
        match self {
            D3dx9Layer::Native => "native",
            D3dx9Layer::Builtin => "builtin",
        }
    }

    /// قراءة من نص JSON.
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "native" | "ms" | "microsoft" => Some(D3dx9Layer::Native),
            "builtin" | "wine" | "wined3d" => Some(D3dx9Layer::Builtin),
            _ => None,
        }
    }
}

/// مُصرّف الشيدرات (d3dcompiler_43.dll) — الأصلي أو المدمج.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compiler {
    /// الأصلي من مايكروسوفت (مطلوب مع D3DX9 الأصلي في ألعاب DX9).
    Native,
    /// المدمج في Wine.
    Builtin,
}

impl Compiler {
    /// المعرّف التقني (نفسه في JSON والواجهة).
    pub fn as_str(self) -> &'static str {
        match self {
            Compiler::Native => "native",
            Compiler::Builtin => "builtin",
        }
    }

    /// قراءة من نص JSON.
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "native" | "ms" | "microsoft" => Some(Compiler::Native),
            "builtin" | "wine" | "wined3d" => Some(Compiler::Builtin),
            _ => None,
        }
    }
}

/// محرك الرسم لـWineD3D: Vulkan (عبر MoltenVK → Metal) أو OpenGL القديم.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Renderer {
    /// `renderer=vulkan` — الطريق الصحيح (Beer's flipflops: يكفي Wine 9+).
    Vulkan,
    /// `renderer=gl` — OpenGL القديم من Apple: بطيء ومكسور مع شيدرات حديثة.
    Gl,
}

impl Renderer {
    /// المعرّف التقني (نفسه في JSON والواجهة) — مطابق لقيمة الريجستري.
    pub fn as_str(self) -> &'static str {
        match self {
            Renderer::Vulkan => "vulkan",
            Renderer::Gl => "gl",
        }
    }

    /// قراءة من نص JSON.
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "vulkan" | "vk" => Some(Renderer::Vulkan),
            "gl" | "opengl" => Some(Renderer::Gl),
            _ => None,
        }
    }
}

// ============================================================
//  الملف الشخصي للعبة
// ============================================================

/// التركيبة الكاملة التي تعمل بها لعبة معيّنة.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameProfile {
    /// رقم اللعبة في ستيم.
    pub appid: u32,
    /// طبقة DirectX 9 (DXVK أو WineD3D).
    pub d3d9: D3d9Layer,
    /// طبقة D3DX9 (`d3dx9_43`).
    pub d3dx9: D3dx9Layer,
    /// مُصرّف الشيدرات (`d3dcompiler_43`).
    pub compiler: Compiler,
    /// محرك الرسم لـWineD3D.
    pub renderer: Renderer,
    /// نفرض تشغيل اللعبة في نافذة (بعض ألعاب DX9 القديمة تاخد الشاشة كلها).
    pub windowed: bool,
    /// Steam overlay مفعّل (هوكل على `IDirect3DDevice9::CreateDevice` ⇒ انهيارات).
    pub overlay: bool,
    /// كم محاولة Déjà جرت على هذه اللعبة.
    pub attempts: u32,
    /// **بصمات التركيبات التي جُرّبت وفشلت** — ما نعيد تجربة تركيبة معروفة
    /// بأنها هي اللي انهارت (كان الثالث محاولة ترجع للتركيبة الأولى).
    pub tried: Vec<String>,
    /// آخر تحديث (بالميلي ثانية) — للعرض ولتنظيف الملفات القديمة.
    pub updated_ms: u64,
}

impl GameProfile {
    /// الافتراضي المعقول: DXVK + D3DX9 الأصلي + مترجم أصلي + Vulkan،
    /// بلا نافذة مفروضة، والـoverlay مفعّل.
    ///
    /// هذا هو التركيب الذي ينجح في أغلب ألعاب DX9 على macOS، ونبدأ منه
    /// ثم نعدّل حسب نتيجة الانهيار (انظر `next_candidate`).
    pub fn default_for(appid: u32) -> Self {
        GameProfile {
            appid,
            d3d9: D3d9Layer::Dxvk,
            d3dx9: D3dx9Layer::Native,
            compiler: Compiler::Native,
            renderer: Renderer::Vulkan,
            windowed: false,
            overlay: true,
            attempts: 0,
            tried: Vec::new(),
            updated_ms: now_ms(),
        }
    }

    /// بصمة التركيبة (نص قصير) — نستخدمها لمقارنة التركيبات ولمنع تكرار
    /// تركيبة جرت وفشلت.
    pub fn signature(&self) -> String {
        format!(
            "d3d9={};d3dx9={};compiler={};renderer={};windowed={};overlay={}",
            self.d3d9.as_str(),
            self.d3dx9.as_str(),
            self.compiler.as_str(),
            self.renderer.as_str(),
            self.windowed,
            self.overlay
        )
    }

    /// هل جرّبنا هذه التركيبة قبلاً وفشلت؟
    pub fn was_tried(&self, signature: &str) -> bool {
        self.tried.iter().any(|t| t == signature)
    }

    /// قيم الريجستري (dll, "native"|"builtin") بالترتيب: d3d9, d3dx9_43, d3dcompiler_43.
    ///
    /// Wine يقرأ `HKCU\Software\Wine\DllOverrides` ⇒ "native" يحمّل ملفاتنا
    /// (DXVK / originals من مايكروسوفت)، و"builtin" يرجع لنسخة Wine المدمجة.
    pub fn registry_overrides(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            (
                "d3d9",
                match self.d3d9 {
                    D3d9Layer::Dxvk => "native",
                    D3d9Layer::WineD3D => "builtin",
                },
            ),
            (
                "d3dx9_43",
                match self.d3dx9 {
                    D3dx9Layer::Native => "native",
                    D3dx9Layer::Builtin => "builtin",
                },
            ),
            (
                "d3dcompiler_43",
                match self.compiler {
                    Compiler::Native => "native",
                    Compiler::Builtin => "builtin",
                },
            ),
        ]
    }

    /// التركيبة التالية بعد انهيار — تُشتق من `action_token` من وحدة triage.
    ///
    /// الرموز المدعومة:
    /// `install-d3dx9-native` · `try-d3dx9-builtin` · `enable-dxvk` ·
    /// `try-wined3d` · `renderer-vulkan` · `renderer-gl` · `disable-overlay` ·
    /// `enable-overlay` · `check-game-files` · `windowed-off`
    ///
    /// ترجع `None` إذا ما فيه تغيير منطقي — مثل `missing-dll:foo.dll`
    /// (يحتاج تدخل المستخدم: نطلب منه تثبيت الملف) أو رمز غير معروف،
    /// أو إذا كانت التركيبة الحالية تحقّق المطلوب أصلاً (ما نبيّن
    /// نعيد نفس التركيبة ونعدّ محاولة بلا فايدة).
    ///
    /// النتيجة: نفس `appid`، مع `attempts` زائد واحد ووقت محدّث.
    pub fn next_candidate(&self, action_token: &str) -> Option<GameProfile> {
        let mut next = self.clone();
        match action_token.trim() {
            // D3DX9 الأصلي (+ المترجم الأصلي): مقاس واحد — أي واحد منهم
            // مدمج يعني إننا لسنا على التركيبة الكاملة.
            "install-d3dx9-native" => {
                if self.d3dx9 == D3dx9Layer::Native && self.compiler == Compiler::Native {
                    return None;
                }
                next.d3dx9 = D3dx9Layer::Native;
                next.compiler = Compiler::Native;
            }
            // العكس: نسخ Wine المدمجة (بعض الألعاب تحتاجها بدل الأصلية).
            "try-d3dx9-builtin" => {
                if self.d3dx9 == D3dx9Layer::Builtin && self.compiler == Compiler::Builtin {
                    return None;
                }
                next.d3dx9 = D3dx9Layer::Builtin;
                next.compiler = Compiler::Builtin;
            }
            // تفعيل/تعطيل DXVK: مصدر شائع لشاشة سودا في DX9.
            "enable-dxvk" => {
                if self.d3d9 == D3d9Layer::Dxvk {
                    return None;
                }
                next.d3d9 = D3d9Layer::Dxvk;
            }
            "try-wined3d" => {
                if self.d3d9 == D3d9Layer::WineD3D {
                    return None;
                }
                next.d3d9 = D3d9Layer::WineD3D;
            }
            // محرك الرسم لـWineD3D: vulkan هو الصحيح على macOS.
            "renderer-vulkan" => {
                if self.renderer == Renderer::Vulkan {
                    return None;
                }
                next.renderer = Renderer::Vulkan;
            }
            "renderer-gl" => {
                if self.renderer == Renderer::Gl {
                    return None;
                }
                next.renderer = Renderer::Gl;
            }
            // الـoverlay: هوكل على IDirect3DDevice9 — سبب معروف لانهيارات DX9.
            "disable-overlay" => {
                if !self.overlay {
                    return None;
                }
                next.overlay = false;
            }
            "enable-overlay" => {
                if self.overlay {
                    return None;
                }
                next.overlay = true;
            }
            // ألعاب DX9 القديمة تاخد الشاشة كلها ⇒ نجرّب نافذة مفروضة.
            "check-game-files" => {
                if self.windowed {
                    return None;
                }
                next.windowed = true;
            }
            "windowed-off" => {
                if !self.windowed {
                    return None;
                }
                next.windowed = false;
            }
            // "missing-dll:…" وما شابه ⇒ يحتاج تدخل المستخدم (لا تغيير آلي).
            _ => return None,
        }
        next.attempts = self.attempts.saturating_add(1);
        next.updated_ms = now_ms();
        // ما نعيد تركيبة جرت وفشلت (المحاولة الثالثة كانت ترجع للأولى)
        if self.was_tried(&next.signature()) {
            return None;
        }
        // ونسجّل التركيبة الحالية كـ«جرت» في(Result) القادم
        let current = self.signature();
        if !next.tried.iter().any(|t| *t == current) {
            next.tried.push(current);
        }
        Some(next)
    }

    /// وصف عربي مختصر للواجهة، مثل: "DXVK + D3DX9 أصلي + Vulkan".
    ///
    /// نبيّن المُترجم فقط إذا اختلف عن D3DX9 (لأنهما يتغيّران معاً عادةً
    /// فمشروحين يختصرون الواجهة).
    pub fn describe_ar(&self) -> String {
        let mut parts: Vec<&str> = Vec::new();
        parts.push(match self.d3d9 {
            D3d9Layer::Dxvk => "DXVK",
            D3d9Layer::WineD3D => "WineD3D",
        });
        parts.push(match self.d3dx9 {
            D3dx9Layer::Native => "D3DX9 أصلي",
            D3dx9Layer::Builtin => "D3DX9 مدمج",
        });
        let compiler_matches = matches!(
            (self.d3dx9, self.compiler),
            (D3dx9Layer::Native, Compiler::Native) | (D3dx9Layer::Builtin, Compiler::Builtin)
        );
        if !compiler_matches {
            parts.push(match self.compiler {
                Compiler::Native => "مترجم أصلي",
                Compiler::Builtin => "مترجم مدمج",
            });
        }
        parts.push(match self.renderer {
            Renderer::Vulkan => "Vulkan",
            Renderer::Gl => "OpenGL",
        });
        parts.join(" + ")
    }

    /// تمثيل JSON (نفسه اللي يُكتب في الملف ويُرسل للواجهة).
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "version": PROFILE_VERSION,
            "appid": self.appid,
            "d3d9": self.d3d9.as_str(),
            "d3dx9": self.d3dx9.as_str(),
            "compiler": self.compiler.as_str(),
            "renderer": self.renderer.as_str(),
            "windowed": self.windowed,
            "overlay": self.overlay,
            "attempts": self.attempts,
            "tried": self.tried,
            "updated_ms": self.updated_ms,
        })
    }

    /// قراءة من JSON — ترجع `None` إذا كان الملف غير صالح.
    ///
    /// **ما نرفضه:** مو كائن، أو `appid` ناقص/مو رقم/صفر/خارج مدى u32
    /// (رقم اللعبة لازم يكون صحيحاً وإلا ما نعرف أي ملف نكتب).
    /// **ما نتسامح معه:** حقول زائدة ما نعرفها (تطوّر لاحقاً)، وحقول
    /// طبقة ناقصة أو بقيمة غير معروفة ⇒ نرجع للافتراضي بدل ما نرمي الملف.
    pub fn from_json(v: &serde_json::Value) -> Option<Self> {
        let obj = v.as_object()?;
        let appid = obj.get("appid").and_then(serde_json::Value::as_u64)?;
        if appid == 0 || appid > u64::from(u32::MAX) {
            return None;
        }
        let appid = appid as u32;
        let fallback = GameProfile::default_for(appid);
        Some(GameProfile {
            appid,
            d3d9: obj
                .get("d3d9")
                .and_then(serde_json::Value::as_str)
                .and_then(D3d9Layer::parse)
                .unwrap_or(fallback.d3d9),
            d3dx9: obj
                .get("d3dx9")
                .and_then(serde_json::Value::as_str)
                .and_then(D3dx9Layer::parse)
                .unwrap_or(fallback.d3dx9),
            compiler: obj
                .get("compiler")
                .and_then(serde_json::Value::as_str)
                .and_then(Compiler::parse)
                .unwrap_or(fallback.compiler),
            renderer: obj
                .get("renderer")
                .and_then(serde_json::Value::as_str)
                .and_then(Renderer::parse)
                .unwrap_or(fallback.renderer),
            windowed: obj
                .get("windowed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(fallback.windowed),
            overlay: obj
                .get("overlay")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(fallback.overlay),
            attempts: obj
                .get("attempts")
                .and_then(serde_json::Value::as_u64)
                .map(|a| u32::try_from(a).unwrap_or(u32::MAX))
                .unwrap_or(fallback.attempts),
            tried: obj
                .get("tried")
                .and_then(serde_json::Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .take(16)
                        .collect()
                })
                .unwrap_or_else(|| fallback.tried.clone()),
            updated_ms: obj
                .get("updated_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(fallback.updated_ms),
        })
    }

    /// قراءة من JSON **مع** التأكد أن `appid` هو نفسه المطلوب.
    ///
    /// نستخدمها عند التحميل: لو الملف تبع لعبة ثانية (رقمها غير مطابق)
    /// نعتبره تالفاً — ما نخلّي ملف لعبة A يغيّر إعدادات لعبة B.
    pub fn from_json_for(appid: u32, v: &serde_json::Value) -> Option<Self> {
        let p = GameProfile::from_json(v)?;
        if p.appid != appid {
            return None;
        }
        Some(p)
    }
}

// ============================================================
//  التخزين — ملف لكل لعبة
// ============================================================

/// مجلد بيانات جسر (نلفّها بدالة عشان نقدر نختبر كل المنطق في مسار مؤقت).
fn base_dir() -> PathBuf {
    crate::data_dir()
}

/// مجلد الملفات الشخصية: <data_dir>/profiles
pub fn profiles_dir() -> PathBuf {
    base_dir().join(PROFILE_DIR_NAME)
}

/// مجلد الملفات الشخصية داخل مجلد أساس معيّن (للاختبار).
fn profiles_dir_in(base: &Path) -> PathBuf {
    base.join(PROFILE_DIR_NAME)
}

/// مسار ملف لعبة واحدة داخل مجلد أساس معيّن.
fn profile_path_in(base: &Path, appid: u32) -> PathBuf {
    profiles_dir_in(base).join(format!("{appid}.json"))
}

/// اقرأ ملف تعريف لعبة (من مجلد أساس معيّن).
///
/// ترجع `None` إذا الملف ما موجود أو تالف — **ما نخترع قيم**.
fn load_in(base: &Path, appid: u32) -> Option<GameProfile> {
    let path = profile_path_in(base, appid);
    let text = std::fs::read_to_string(&path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    GameProfile::from_json_for(appid, &value)
}

/// احفظ ملف تعريف لعبة (في مجلد أساس معيّن) — كتابة ذرّية.
///
/// نكتب في ملف مؤقت ثم نعيد تسميته: لو انقطعت العملية في النص ما يبقى
/// عندنا ملف ناقص (نص/نص) يفسّر الملف الشخصي.
fn save_in(base: &Path, profile: &GameProfile) -> Result<(), String> {
    let dir = profiles_dir_in(base);
    std::fs::create_dir_all(&dir).map_err(|e| format!("تعذّر إنشاء مجلد الملفات الشخصية: {e}"))?;
    let path = dir.join(format!("{}.json", profile.appid));
    let tmp = dir.join(format!("{}.json.tmp", profile.appid));
    let text = serde_json::to_string_pretty(&profile.to_json())
        .map_err(|e| format!("تعذّر تجهيز محتوى الملف الشخصي: {e}"))?;
    std::fs::write(&tmp, text.as_bytes()).map_err(|e| format!("تعذّر كتابة الملف الشخصي: {e}"))?;
    // إعادة التسمية هي الخطوة الذرّية (تستبدل الملف القديم دفعة واحدة).
    if let Err(first) = std::fs::rename(&tmp, &path) {
        // بعض أنظمة الملفات ترفض الاستبدال المباشر — نحذف القديم ونعيد.
        if path.exists() && std::fs::remove_file(&path).is_ok() {
            std::fs::rename(&tmp, &path).map_err(|e| format!("تعذّر حفظ الملف الشخصي: {e}"))?;
        } else {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("تعذّر حفظ الملف الشخصي: {first}"));
        }
    }
    Ok(())
}

/// احذف ملف تعريف لعبة (في مجلد أساس معيّن).
///
/// حذف ملف غير موجود = نجاح (العملية idempotent: المستخدم يقدر يضغط
/// «تصفير» بلا ما نخيفه بخطأ).
fn clear_in(base: &Path, appid: u32) -> Result<(), String> {
    let path = profile_path_in(base, appid);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("تعذّر حذف الملف الشخصي: {e}")),
    }
}

/// اقرأ ملف تعريف اللعبة المحفوظ (ولو ما موجود ⇒ `None`).
pub fn load(appid: u32) -> Option<GameProfile> {
    load_in(&base_dir(), appid)
}

/// احفظ ملف تعريف اللعبة (التركيبة التي نجحت معها).
pub fn save(profile: &GameProfile) -> Result<(), String> {
    save_in(&base_dir(), profile)
}

/// احذف ملف تعريف اللعبة (رجوع للافتراضي).
pub fn clear(appid: u32) -> Result<(), String> {
    clear_in(&base_dir(), appid)
}

/// الوقت الحالي بالميلي ثانية (من حقبة يونكس).
fn now_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => u64::try_from(d.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

// ============================================================
//  الاختبارات
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// مجلد مؤقت نظيف لكل اختبار (نتجنّب التلويث بين الاختبارات).
    fn temp_dir(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("jisr-combo-{tag}-{stamp}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("temp dir: {e}"));
        dir
    }

    #[test]
    fn default_for_is_the_sane_combination() {
        let p = GameProfile::default_for(440);
        assert_eq!(p.appid, 440);
        assert_eq!(p.d3d9, D3d9Layer::Dxvk);
        assert_eq!(p.d3dx9, D3dx9Layer::Native);
        assert_eq!(p.compiler, Compiler::Native);
        assert_eq!(p.renderer, Renderer::Vulkan);
        assert!(!p.windowed);
        assert!(p.overlay);
        assert_eq!(p.attempts, 0);
        assert!(p.updated_ms > 0, "الوقت لازم يكون مضبوط");
    }

    #[test]
    fn registry_overrides_cover_every_value() {
        let mut p = GameProfile::default_for(1);
        let ov = p.registry_overrides();
        assert_eq!(
            ov,
            vec![
                ("d3d9", "native"),
                ("d3dx9_43", "native"),
                ("d3dcompiler_43", "native")
            ]
        );

        p.d3d9 = D3d9Layer::WineD3D;
        p.d3dx9 = D3dx9Layer::Builtin;
        p.compiler = Compiler::Builtin;
        assert_eq!(
            p.registry_overrides(),
            vec![
                ("d3d9", "builtin"),
                ("d3dx9_43", "builtin"),
                ("d3dcompiler_43", "builtin")
            ]
        );
    }

    #[test]
    fn d3dx9_native_token_moves_both_layers() {
        let mut p = GameProfile::default_for(1);
        p.d3dx9 = D3dx9Layer::Builtin;
        p.compiler = Compiler::Builtin;
        let c = p.next_candidate("install-d3dx9-native").expect("فيه تغيير");
        assert_eq!(c.d3dx9, D3dx9Layer::Native);
        assert_eq!(c.compiler, Compiler::Native);
        assert_eq!(c.attempts, p.attempts + 1);
        assert_eq!(c.appid, p.appid);
        assert!(c.updated_ms >= p.updated_ms);
        // ما فيه تغيير ⇒ ما فيه تركيبة جديدة
        assert!(c.next_candidate("install-d3dx9-native").is_none());
    }

    #[test]
    fn d3dx9_builtin_token_moves_both_layers() {
        let p = GameProfile::default_for(1);
        let c = p.next_candidate("try-d3dx9-builtin").expect("فيه تغيير");
        assert_eq!(c.d3dx9, D3dx9Layer::Builtin);
        assert_eq!(c.compiler, Compiler::Builtin);
        assert_eq!(c.attempts, 1);
        assert!(c.next_candidate("try-d3dx9-builtin").is_none());
    }

    #[test]
    fn dxvk_and_wined3d_tokens_switch_d3d9() {
        let mut p = GameProfile::default_for(1);
        assert!(p.next_candidate("enable-dxvk").is_none(), "DXVK مفعّل أصلاً");
        p.d3d9 = D3d9Layer::WineD3D;
        let c = p.next_candidate("enable-dxvk").expect("فيه تغيير");
        assert_eq!(c.d3d9, D3d9Layer::Dxvk);

        let q = GameProfile::default_for(1);
        let c2 = q.next_candidate("try-wined3d").expect("فيه تغيير");
        assert_eq!(c2.d3d9, D3d9Layer::WineD3D);
        assert!(c2.next_candidate("try-wined3d").is_none());
    }

    #[test]
    fn renderer_tokens_switch_the_backend() {
        let mut p = GameProfile::default_for(1);
        assert!(p.next_candidate("renderer-vulkan").is_none(), "Vulkan أصلاً");
        p.renderer = Renderer::Gl;
        let c = p.next_candidate("renderer-vulkan").expect("فيه تغيير");
        assert_eq!(c.renderer, Renderer::Vulkan);
        assert!(c.next_candidate("renderer-vulkan").is_none());

        let q = GameProfile::default_for(1);
        let c2 = q.next_candidate("renderer-gl").expect("فيه تغيير");
        assert_eq!(c2.renderer, Renderer::Gl);
        assert!(c2.next_candidate("renderer-gl").is_none());
    }

    #[test]
    fn overlay_tokens_toggle_the_hook() {
        let p = GameProfile::default_for(1);
        assert!(p.overlay, "الـoverlay مفعّل افتراضياً");
        let off = p.next_candidate("disable-overlay").expect("فيه تغيير");
        assert!(!off.overlay);
        assert!(
            off.next_candidate("disable-overlay").is_none(),
            "الـoverlay معطّل أصلاً — ما فيه تغيير"
        );
        // الرجوع للـoverlay مفعّل = التركيبة الافتراضية ⇒ جُرّبت؟ لا، لكن
        // «enable-overlay» منTurbo لا ينفع لأنه سيعيدنا لتركيبة معروفة.
        assert!(
            off.next_candidate("enable-overlay").is_none(),
            "ما نرجع لتركيبة مجرّبة"
        );
    }

    #[test]
    fn windowed_tokens_toggle_the_window() {
        let p = GameProfile::default_for(1);
        let c = p.next_candidate("check-game-files").expect("فيه تغيير");
        assert!(c.windowed);
        assert!(c.next_candidate("check-game-files").is_none());
        // الرجوع لغير النافذة = التركيبة الافتراضية (جرت) ⇒ ما نرجع لها
        assert!(
            c.next_candidate("windowed-off").is_none(),
            "ما نرجع لتركيبة مجرّبة"
        );
    }

    /// التركيبات التي جرت وفشلت ما تُعاد — المحاولة الثالثة كانت ترجع للأولى
    #[test]
    fn never_repeats_a_tried_combination() {
        let p = GameProfile::default_for(1);
        let a = p.next_candidate("try-d3dx9-builtin").expect("تركيبة جديدة");
        assert!(a.tried.iter().any(|t| t == &p.signature()));
        // من A: "تثبيت الأصلي" يرجع للتركيبة الافتراضية ⇒ مرفوض
        assert!(a.next_candidate("install-d3dx9-native").is_none());
        // لكن تركيبة جديدة (WineD3D) مقبولة
        assert!(a.next_candidate("try-wined3d").is_some());
    }

    #[test]
    fn missing_dll_and_unknown_tokens_need_a_human() {
        let p = GameProfile::default_for(1);
        assert!(p.next_candidate("missing-dll:foo.dll").is_none());
        assert!(p.next_candidate("install-d3dx9-native-typo").is_none());
        assert!(p.next_candidate("").is_none());
        assert!(p.next_candidate("reinstall-everything").is_none());
    }

    #[test]
    fn json_round_trip_keeps_every_field() {
        let mut p = GameProfile::default_for(1234);
        p.d3d9 = D3d9Layer::WineD3D;
        p.d3dx9 = D3dx9Layer::Builtin;
        p.compiler = Compiler::Builtin;
        p.renderer = Renderer::Gl;
        p.windowed = true;
        p.overlay = false;
        p.attempts = 7;
        let v = p.to_json();
        assert_eq!(v["version"], json!(1));
        assert_eq!(v["appid"], json!(1234));
        assert_eq!(v["d3d9"], json!("wined3d"));
        let back = GameProfile::from_json(&v).expect("ملف صالح");
        assert_eq!(back, p);
    }

    #[test]
    fn from_json_rejects_bad_appid_and_non_object() {
        assert!(GameProfile::from_json(&json!({ "appid": 5 })).is_some());
        assert!(GameProfile::from_json(&json!({})).is_none(), "appid ناقص");
        assert!(
            GameProfile::from_json(&json!({ "appid": "1234" })).is_none(),
            "appid مو رقم"
        );
        assert!(
            GameProfile::from_json(&json!({ "appid": 0 })).is_none(),
            "appid صفر"
        );
        assert!(
            GameProfile::from_json(&json!({ "appid": 9_999_999_999_999u64 })).is_none(),
            "appid خارج مدى u32"
        );
        assert!(
            GameProfile::from_json(&json!([1, 2, 3])).is_none(),
            "مو كائن"
        );
        assert!(GameProfile::from_json(&json!("نص")).is_none());
        // appid مختلف عن المطلوب ⇒ نرفض (ملف لعبة A ما يغيّر إعدادات B)
        let v = GameProfile::default_for(42).to_json();
        assert!(GameProfile::from_json_for(42, &v).is_some());
        assert!(GameProfile::from_json_for(43, &v).is_none());
    }

    #[test]
    fn from_json_tolerates_extra_fields() {
        let mut v = GameProfile::default_for(9).to_json();
        v["future_field"] = json!("xyz");
        v["another"] = json!({ "nested": true });
        let p = GameProfile::from_json(&v).expect("حقول زيادة ما تكسر القراءة");
        assert_eq!(p.appid, 9);
        assert_eq!(p.d3d9, D3d9Layer::Dxvk);
    }

    #[test]
    fn save_load_clear_round_trip() {
        let base = temp_dir("roundtrip");
        let mut p = GameProfile::default_for(77);
        p.attempts = 2;
        p.windowed = true;
        assert!(load_in(&base, 77).is_none(), "ما فيه ملف قبل الحفظ");
        save_in(&base, &p).expect("الحفظ ينجح");
        let loaded = load_in(&base, 77).expect("الملف موجود بعد الحفظ");
        assert_eq!(loaded, p);
        // الملف موجود على القرص ويحلَّل
        let path = profile_path_in(&base, 77);
        assert!(path.is_file());
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let value: serde_json::Value = serde_json::from_str(&text).expect("JSON صالح");
        assert_eq!(value["version"], json!(1));
        assert_eq!(value["attempts"], json!(2));
        // الحفظ الذرّي: ما يبقى ملف مؤقت
        assert!(!dir_has_tmp(&base), "ما يبقى ملف مؤقت بعد الحفظ");
        // الحذف
        clear_in(&base, 77).expect("الحذف ينجح");
        assert!(load_in(&base, 77).is_none());
        // حذف ملف غير موجود = نجاح (idempotent)
        assert!(clear_in(&base, 77).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn corrupt_file_is_ignored_without_panic() {
        let base = temp_dir("corrupt");
        let dir = profiles_dir_in(&base);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("mkdir: {e}"));
        let path = dir.join("55.json");
        std::fs::write(&path, b"{ this is not json").unwrap_or_else(|e| panic!("write: {e}"));
        assert!(load_in(&base, 55).is_none(), "ملف تالف ⇒ None");
        // ملف JSON سليم لكن فيه appid مختلف ⇒ نرفضه (لا خلط بين الألعاب)
        let other = GameProfile::default_for(56).to_json();
        std::fs::write(&path, serde_json::to_string(&other).unwrap_or_default())
            .unwrap_or_else(|e| panic!("write: {e}"));
        assert!(load_in(&base, 55).is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn describe_ar_mentions_the_layers() {
        let p = GameProfile::default_for(1);
        let d = p.describe_ar();
        assert!(d.contains("DXVK"), "{d}");
        assert!(d.contains("D3DX9"), "{d}");
        assert!(d.contains("Vulkan"), "{d}");
        assert_eq!(d, "DXVK + D3DX9 أصلي + Vulkan");

        let mut q = GameProfile::default_for(1);
        q.d3d9 = D3d9Layer::WineD3D;
        q.renderer = Renderer::Gl;
        q.compiler = Compiler::Builtin;
        let d2 = q.describe_ar();
        assert!(d2.contains("WineD3D"), "{d2}");
        assert!(d2.contains("OpenGL"), "{d2}");
        assert!(d2.contains("مترجم"), "{d2}");
    }

    /// هل في ملف مؤقت (*.tmp) داخل المجلد؟
    fn dir_has_tmp(base: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(profiles_dir_in(base)) else {
            return false;
        };
        entries
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".json.tmp"))
    }
}
