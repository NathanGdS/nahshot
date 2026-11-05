#!/usr/bin/env bash
# Tests for scripts/release.sh. Each case builds a throwaway repo with a bare
# "origin", runs the script and checks which tags reached origin.
set -u
SCRIPT="$(cd "$(dirname "$0")" && pwd)/release.sh"
pass=0
fail=0

setup() {
  WORK=$(mktemp -d)
  git init -q --bare -b main "$WORK/origin.git"
  git clone -q "$WORK/origin.git" "$WORK/repo" 2>/dev/null
  cd "$WORK/repo"
  git config user.email t@t
  git config user.name t
  git commit -q --allow-empty -m init
  git push -q origin main
}

remote_tags() { git ls-remote --tags --refs origin | sed 's#.*refs/tags/##' | sort | tr '\n' ' '; }

check() { # name, condition result (0 ok)
  if [ "$2" -eq 0 ]; then pass=$((pass + 1)); echo "ok   $1"; else fail=$((fail + 1)); echo "FAIL $1"; fi
}

# first release defaults to v0.1.0
setup
bash "$SCRIPT" --yes >/dev/null 2>&1
[ "$(remote_tags)" = "v0.1.0 " ]; check "first release is v0.1.0" $?

# default bump is patch over the latest pushed tag
setup
git tag v0.1.0 && git push -q origin v0.1.0
git commit -q --allow-empty -m x
bash "$SCRIPT" --yes >/dev/null 2>&1
[ "$(remote_tags)" = "v0.1.0 v0.1.1 " ]; check "patch bump from latest tag" $?

# minor / major keywords
setup
git tag v1.2.3 && git push -q origin v1.2.3
git commit -q --allow-empty -m x
bash "$SCRIPT" minor --yes >/dev/null 2>&1
[ "$(remote_tags)" = "v1.2.3 v1.3.0 " ]; check "minor bump" $?
git commit -q --allow-empty -m y
bash "$SCRIPT" major --yes >/dev/null 2>&1
[ "$(remote_tags)" = "v1.2.3 v1.3.0 v2.0.0 " ]; check "major bump" $?

# explicit version, with or without "v", prerelease allowed
setup
bash "$SCRIPT" 2.0.0-beta.1 --yes >/dev/null 2>&1
bash "$SCRIPT" v2.0.0 --yes >/dev/null 2>&1
[ "$(remote_tags)" = "v2.0.0 v2.0.0-beta.1 " ]; check "explicit versions" $?

# latest is read from origin, not from local-only tags
setup
git tag v9.9.9
bash "$SCRIPT" --yes >/dev/null 2>&1
[ "$(remote_tags)" = "v0.1.0 " ]; check "ignores tags that were never pushed" $?

# bump ignores prereleases when picking the base
setup
git tag v1.0.0 && git tag v1.1.0-beta.1 && git push -q origin --tags
bash "$SCRIPT" --yes >/dev/null 2>&1
echo "$(remote_tags)" | grep -q "v1.0.1"; check "patch bump uses latest stable tag" $?

# unpushed commits on main are pushed before the tag
setup
git commit -q --allow-empty -m "local work"
bash "$SCRIPT" --yes >/dev/null 2>&1
[ "$(git rev-parse origin/main)" = "$(git rev-parse HEAD)" ] && [ "$(git rev-parse 'v0.1.0^{commit}')" = "$(git rev-parse HEAD)" ]
check "pushes main before tagging" $?

# refusals
setup
git tag v0.1.0 && git push -q origin v0.1.0
bash "$SCRIPT" 0.1.0 --yes >/dev/null 2>&1; [ $? -ne 0 ]; check "rejects existing tag" $?
bash "$SCRIPT" 1.2 --yes >/dev/null 2>&1; [ $? -ne 0 ]; check "rejects invalid version" $?
echo dirty > f.txt && git add f.txt
bash "$SCRIPT" --yes >/dev/null 2>&1; [ $? -ne 0 ]; check "rejects dirty tree" $?
git reset -q --hard
git checkout -q -b feature
bash "$SCRIPT" --yes >/dev/null 2>&1; [ $? -ne 0 ]; check "rejects branch other than main" $?
git checkout -q main
[ "$(remote_tags)" = "v0.1.0 " ]; check "refusals create no tags" $?

# behind origin
setup
git clone -q "$WORK/origin.git" "$WORK/other" && (cd "$WORK/other" && git -c user.email=t@t -c user.name=t commit -q --allow-empty -m remote && git push -q origin main)
bash "$SCRIPT" --yes >/dev/null 2>&1; [ $? -ne 0 ]; check "rejects when behind origin" $?

# refuses to run inside WSL, where git has no Windows credentials
setup
echo "Linux version 5.15.167.4-microsoft-standard-WSL2" > "$WORK/proc_version"
out=$(NAHSHOT_PROC_VERSION="$WORK/proc_version" bash "$SCRIPT" --yes 2>&1); rc=$?
[ $rc -ne 0 ] && [ "$(remote_tags)" = "" ] && echo "$out" | grep -q "WSL"; check "refuses to run under WSL" $?

# dry run changes nothing
setup
out=$(bash "$SCRIPT" --dry-run 2>&1)
[ "$(remote_tags)" = "" ] && [ -z "$(git tag -l)" ] && echo "$out" | grep -q "v0.1.0"; check "dry run only prints" $?

# without --yes it asks and a "n" aborts
setup
echo n | bash "$SCRIPT" >/dev/null 2>&1
[ "$(remote_tags)" = "" ]; check "answering no aborts" $?

echo
echo "$pass ok, $fail failed"
[ "$fail" -eq 0 ]
