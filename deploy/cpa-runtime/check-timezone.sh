#!/bin/sh
# Validate the published runtime with host-style tzfile mounts and no TZ override.
set -eu
image_ref="${1:?usage: check-timezone.sh IMAGE}"
docker_bin="${DOCKER:-docker}"
fixture_dir="$(mktemp -d)"
fixture_dir="$(cd "$fixture_dir" && pwd -P)"
chmod 755 "$fixture_dir"
trap 'rm -f "$fixture_dir"/*.localtime "$fixture_dir"/*.timezone; rmdir "$fixture_dir"' EXIT HUP INT TERM

"$docker_bin" run --rm --entrypoint sh "$image_ref" -c 'test -f /etc/localtime && test ! -L /etc/localtime'
for fixture_zone in Etc/UTC Asia/Singapore Asia/Tokyo; do
    fixture_localtime="$fixture_dir/${fixture_zone##*/}.localtime"
    fixture_timezone="$fixture_dir/${fixture_zone##*/}.timezone"
    case "$fixture_zone" in
        Etc/UTC) expected_offset=+0000 ;;
        Asia/Singapore) expected_offset=+0800 ;;
        Asia/Tokyo) expected_offset=+0900 ;;
    esac
    "$docker_bin" run --rm --entrypoint cat "$image_ref" "/usr/share/zoneinfo/$fixture_zone" > "$fixture_localtime"
    chmod 644 "$fixture_localtime"
    # shellcheck disable=SC2016 # TZ must be checked inside the container.
    actual_offset="$("$docker_bin" run --rm \
        --mount "type=bind,source=$fixture_localtime,target=/etc/localtime,readonly" \
        --entrypoint sh "$image_ref" -ec 'test -z "${TZ-}"; date +%z; TZ=Etc/UTC date +%z')"
    test "$actual_offset" = "$(printf '%s\n+0000' "$expected_offset")"
    printf '%s\n' "$fixture_zone" > "$fixture_timezone"
    chmod 644 "$fixture_timezone"
    named_offset="$("$docker_bin" run --rm \
        --mount "type=bind,source=$fixture_localtime,target=/etc/localtime,readonly" \
        --mount "type=bind,source=$fixture_timezone,target=/etc/timezone,readonly" \
        --entrypoint sh "$image_ref" -c 'cat /etc/timezone; date +%z')"
    test "$named_offset" = "$(printf '%s\n%s' "$fixture_zone" "$expected_offset")"
    printf 'PASS host timezone mount: %s (%s), TZ unset, UTC database unchanged\n' "$fixture_zone" "$expected_offset"
done
