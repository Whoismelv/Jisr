#!/usr/bin/env bash
# check-licenses.sh — بوابة «كل شيء مجاني ومفتوح»
#
# تُفشل النشر إذا:
#   1) أي مكوّن خارجي جُلب بلا ملفات ترخيصه (شرط LGPL/GPL/Apache/MIT)
#   2) كودنا يتصل بخوادم CodeWeavers (استقلال كامل)
#   3) كودنا فيه مفاهيم ترخيص/تفعيل (serial/activation/license server)
#   4) مستودعنا فيه بايتات مملوكة لهم (CrossTie/بايت-كود/D3DMetal)
#   5) ترخيص التطبيق ليس MIT
#   6) تبعيات Rust برخص copyleft مقيّدة داخل الـlock
#
# التشغيل: bash vendor/check-licenses.sh
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAIL=0
ok()   { printf '\033[1;32m✔\033[0m %s\n' "$*"; }
bad()  { printf '\033[1;31m✘\033[0m %s\n' "$*"; FAIL=1; }
note() { printf '\033[1;34m▸\033[0m %s\n' "$*"; }

cd "${ROOT}"

# ---------------------------------------------------------------- 1) التراخيص
note "1) ملفات ترخيص المكوّنات الخارجية"
if [ -d vendor/src ]; then
  while IFS= read -r dir; do
    name="$(basename "${dir}")"
    if ls "${dir}"/LICENSE* "${dir}"/COPYING* >/dev/null 2>&1; then
      ok "${name}: ملف الترخيص موجود"
    else
      bad "${name}: لا يوجد LICENSE/COPYING — لا تنشر بدونه (GPL/LGPL/Apache تفرض بقاءه)"
    fi
  done < <(find vendor/src -mindepth 1 -maxdepth 1 -type d)
else
  note "vendor/src غير موجود (لم تشغّل fetch-sources.sh بعد) — تخطّي"
fi

# ------------------------------------------- 2) لا اتصالات بخوادم CodeWeavers
note "2) صفر اتصال بخوادم CodeWeavers"
URLS=$(grep -rIn --binary-files=without-match \
        -e "codeweavers\.com" \
        -e "codeweavers\.net" \
        src src-tauri/src src-tauri/resources 2>/dev/null || true)
if [ -z "${URLS}" ]; then
  ok "لا روابط لخوادم CodeWeavers في كودنا"
else
  bad "روابط CodeWeavers داخل الكود (المنتج مستقل — نمسحها):"
  echo "${URLS}" | head -n 20
fi

# ------------------------------------------------------- 3) لا مفاهيم ترخيص
note "3) صفر مفاهيم ترخيص/تفعيل في منطق التطبيق"
LIC_HITS=$(grep -rIn --binary-files=without-match \
        -e "license server" -e "license_server" \
        -e "activation code" -e "activation_code" \
        -e "register_serial" -e "serial_code" \
        src-tauri/src 2>/dev/null || true)
if [ -z "${LIC_HITS}" ]; then
  ok "لا يوجد خادم ترخيص ولا أكواد تفعيل"
else
  bad "وجدنا مفاهيم ترخيص في المنطق (المنتج مجاني بلا تسجيل):"
  echo "${LIC_HITS}" | head -n 20
fi

# ------------------------------------- 4) لا بايتات مملوكة داخل المستودع
note "4) لا بايتات مملوكة لهم داخل المستودع"
PROP=$(find . -path ./node_modules -prune -o -path ./src-tauri/target -prune -o \
        -path ./.git -prune -o \
        \( -name "*.pyc" -o -name "D3DMetal.framework" \
           -o -name "crossover.tie" -o -name "unknown.tie" \) -print 2>/dev/null || true)
# ملفات .c4p الكبيرة = قاعدة توافق جاهزة (قاعدة CodeWeavers 23MB) — نترك الصغيرة (بذرنا)
PROP2=$(find src-tauri/resources -name "*.c4p" -size +1M -print 2>/dev/null || true)
if [ -z "${PROP}" ] && [ -z "${PROP2}" ]; then
  ok "لا ملفات .pyc / crossover.tie / D3DMetal داخل المستودع"
else
  bad "ملفات مملوكة لـ CodeWeavers/Apple داخل المستودع (شيلها):"
  [ -n "${PROP}" ] && echo "${PROP}" | head -n 20
  [ -n "${PROP2}" ] && echo "${PROP2}" | head -n 20
fi

# ------------------------------------------------------- 5) ترخيص التطبيق
note "5) ترخيص التطبيق"
if [ -f LICENSE ] && head -n 3 LICENSE | grep -qi "MIT License"; then
  ok "LICENSE: MIT"
else
  bad "ملف LICENSE ناقص أو ليس MIT"
fi

# --------------------------------------------------- 6) تبعيات Rust
note "6) تبعيات Rust — رخصها معروفة ومسموحة"
if [ -f src-tauri/Cargo.lock ]; then
  BADLIC=$(grep -E '^name = |^license = ' src-tauri/Cargo.lock \
           | paste - - 2>/dev/null \
           | grep -Ei "license = \"(GPL|AGPL|SSPL|BSL|Elastic|Commons-Clause)" || true)
  if [ -z "${BADLIC}" ]; then
    ok "لا تبعيات Rust برخص copyleft/مقيّدة داخل الـlock"
  else
    bad "تبعيات Rust برخص مقيّدة (انسخها من المصدر بدل الـlock):"
    echo "${BADLIC}" | head -n 10
  fi
else
  note "لا يوجد Cargo.lock — تخطّي"
fi

# ------------------------------------------------------------ 7) بيان المصادر
if [ -f vendor/SOURCES.md ]; then
  ok "vendor/SOURCES.md موجود (بيان المكوّنات + تراخيصها)"
else
  note "vendor/SOURCES.md غير موجود — أنشئه بـ fetch-sources.sh"
fi

echo
if [ "${FAIL}" -eq 0 ]; then
  printf '\033[1;32m══ كل الفحوصات نجحت: المشروع مجاني ومفتوح بالكامل ══\033[0m\n'
else
  printf '\033[1;31m══ فشلت بوابة التراخيص — لا تنشر ══\033[0m\n'
fi
exit "${FAIL}"
