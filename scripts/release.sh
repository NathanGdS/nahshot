#!/usr/bin/env bash
# Creates the next version tag and pushes it to origin. Pushing a v* tag is what
# triggers .github/workflows/release.yml, which builds the installer and publishes
# the GitHub release.
#
# Usage: scripts/release.sh [patch|minor|major|X.Y.Z[-pre]] [--yes] [--dry-run]
#   (no version)  next patch after the latest stable tag on origin
#   --yes         don't ask for confirmation
#   --dry-run     only print what would happen
set -euo pipefail

# Git for Windows started from some tools has no HOME, so git misses ~/.gitconfig.
export HOME="${USERPROFILE:-$HOME}"

BRANCH=main
bump=patch
yes=0
dry=0
for arg in "$@"; do
  case "$arg" in
    --yes | -y) yes=1 ;;
    --dry-run) dry=1 ;;
    -h | --help)
      sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) bump="$arg" ;;
  esac
done

die() { echo "Erro: $*" >&2; exit 1; }

semver_re='^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$'

# From PowerShell, plain "bash" is WSL's bash: git there has no Windows credentials, so
# the push would ask for user/password (and GitHub rejects passwords anyway).
if grep -qi microsoft "${NAHSHOT_PROC_VERSION:-/proc/version}" 2>/dev/null; then
  die "você está no bash do WSL, onde o git não tem suas credenciais do Windows. Rode pelo 'make release' ou pelo Git Bash."
fi

[ "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" ] ||
  die "rode a partir da branch $BRANCH (você está em $(git rev-parse --abbrev-ref HEAD))."
git diff --quiet && git diff --cached --quiet ||
  die "há mudanças não commitadas. Faça commit ou stash antes."

git fetch -q origin "$BRANCH"
behind=$(git rev-list --count "HEAD..origin/$BRANCH")
ahead=$(git rev-list --count "origin/$BRANCH..HEAD")
[ "$behind" -eq 0 ] || die "sua $BRANCH está $behind commit(s) atrás do origin. Rode git pull antes."

# Tags that exist on origin (the ones CI has seen), newest stable first.
remote_tags=$(git ls-remote --tags --refs origin 'v*' | sed 's#.*refs/tags/##')
latest_stable=$(echo "$remote_tags" | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sed 's/^v//' | sort -t. -k1,1n -k2,2n -k3,3n | tail -n1 || true)

case "$bump" in
  patch | minor | major)
    if [ -z "$latest_stable" ]; then
      version=0.1.0
    else
      IFS=. read -r ma mi pa <<<"$latest_stable"
      case "$bump" in
        patch) version="$ma.$mi.$((pa + 1))" ;;
        minor) version="$ma.$((mi + 1)).0" ;;
        major) version="$((ma + 1)).0.0" ;;
      esac
    fi
    ;;
  *) version="${bump#v}" ;;
esac

[[ "$version" =~ $semver_re ]] || die "'$bump' não é uma versão válida (ex.: 1.2.0 ou 1.2.0-beta.1)."
tag="v$version"
echo "$remote_tags" | grep -qx "$tag" && die "a tag $tag já existe no origin."
git rev-parse -q --verify "refs/tags/$tag" >/dev/null && die "a tag $tag já existe localmente. Apague com: git tag -d $tag"

echo "Última versão no origin: ${latest_stable:+v$latest_stable}${latest_stable:-nenhuma}"
echo "Nova tag:                $tag  ($(git rev-parse --short HEAD) $(git log -1 --format=%s))"
[ "$ahead" -gt 0 ] && echo "Antes, vai enviar $ahead commit(s) da $BRANCH para o origin."

if [ "$dry" -eq 1 ]; then
  echo "(dry-run: nada foi feito)"
  exit 0
fi

if [ "$yes" -ne 1 ]; then
  read -r -p "Criar e enviar $tag? [s/N] " answer || answer=""
  case "$answer" in s | S | sim | y | Y) ;; *) echo "Cancelado."; exit 1 ;; esac
fi

[ "$ahead" -gt 0 ] && git push -q origin "$BRANCH"
git tag -a "$tag" -m "Release $tag"
git push -q origin "$tag"

repo_url=$(git remote get-url origin | sed -E 's#^git@github\.com:#https://github.com/#; s#\.git$##')
echo "Pronto: $tag enviada. O GitHub Actions vai buildar o instalador e publicar a release."
case "$repo_url" in https://github.com/*) echo "Acompanhe em $repo_url/actions" ;; esac
