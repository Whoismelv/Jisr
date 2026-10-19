# AGENTS.md — تعليمات الوكلاء على مستودع جسر (Jisr)

قواعد مختصرة لأي مساعد ذكي (OpenCode / Claude / Codex …) يشتغل على هذا المستودع.

## اقرأ أولاً (إلزامي)

- **`PROJECT.md`** — ذاكرة المشروع: القرارات، الدروس، حالة الجلسات، بوابات الجودة.
- **`README.md`** — وصف المستخدم النهائي، و**`docs/COMPARISON.md`** — المقارنة الصادقة مع Whisky/CrossOver.

## بوابات الجودة قبل أي commit

```bash
# الخلفية (Rust) — من جذر المستودع
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --lib --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml

# الواجهة (React/TS)
npm ci          # عند تغيّر package-lock
npx tsc --noEmit && npm run build
```

التفاصيل والسياق الكامل في `PROJECT.md` (§بوابات الجودة / §قواعد ذهبية).

## الرسم المعرفي (graphify) — أداة تطوير محلية

الرسم لكل المشروع في `graphify-out/` (محلي، خارج git — لا يوجد على غير جهاز التطوير).

**إذا وُجد `graphify-out/graph.json`:**

1. **قبل أي تعديل كود غير تافه** — استشر الرسم بدل التخمين:
   - `python -m graphify query "<سؤال عن الكود>"`
   - `python -m graphify explain "<اسم دالة/ملف/مفهوم>"`
   - `python -m graphify path "<A>" "<B>"`
   - أو افتح `graphify-out/graph.html` / اقرأ `GRAPH_REPORT.md`
2. **بعد تعديل الكود**: لا شيء يدوي — git-hook يحدّث جزء الكود تلقائياً بعد كل commit (AST بلا LLM).
3. **بنهاية كل جلسة** — شغّل:
   ```bash
   python tools/graphify/session_end.py
   ```
   - إن قال «لا تغييرات» → خلص.
   - إن أعلن عن مستندات متغيرة → استخرجها دلالياً (subagent وفق
     `skills/graphify/references/extraction-spec.md`: المصدر = المسارات المعلَنة،
     الناتج = `graphify-out/.graphify_chunk_docs_<N>.json`) ثم:
     ```bash
     python tools/graphify/session_end.py --finish
     ```
4. حدّث `PROJECT.md` بسطر الجلسة (ما تم، القرارات، المعلّق) بعد اكتمال الفحص.

**إذا ما وُجد `graphify-out/`:** تجاهل هذا القسم بالكامل (الأداة غير مثبتة على هذا الجهاز).

### تثبيت الأداة (عند الحاجة، لمرة واحدة)

```bash
python -m pip install --upgrade graphifyy
python -m graphify hook install     # hooks: post-commit/checkout/merge + merge driver
```
