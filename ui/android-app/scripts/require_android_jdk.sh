#!/usr/bin/env bash

aerobag_java_home_from_current_java() {
  local java_bin
  java_bin="$(command -v java 2>/dev/null || true)"
  if [[ -z "$java_bin" ]]; then
    return 1
  fi
  "$java_bin" -XshowSettings:properties -version 2>&1 \
    | sed -n 's/^[[:space:]]*java.home = //p' \
    | head -1
}

aerobag_select_android_jdk() {
  if [[ -n "${JAVA_HOME:-}" ]]; then
    if [[ -x "$JAVA_HOME/bin/java" && -x "$JAVA_HOME/bin/jlink" ]]; then
      export JAVA_HOME
      return 0
    fi
    cat >&2 <<EOF
Android builds require JAVA_HOME to point at a full JDK with jlink.
Current JAVA_HOME is not usable: ${JAVA_HOME}
Install openjdk-21-jdk or set JAVA_HOME to a full JDK such as /usr/lib/jvm/java-21-openjdk-amd64.
EOF
    return 1
  fi

  local candidate
  for candidate in \
    /usr/lib/jvm/java-21-openjdk-amd64 \
    /usr/lib/jvm/java-17-openjdk-amd64
  do
    if [[ -x "$candidate/bin/java" && -x "$candidate/bin/jlink" ]]; then
      export JAVA_HOME="$candidate"
      return 0
    fi
  done

  local current_java_home
  current_java_home="$(aerobag_java_home_from_current_java || true)"
  if [[ -n "$current_java_home" && -x "$current_java_home/bin/java" && -x "$current_java_home/bin/jlink" ]]; then
    export JAVA_HOME="$current_java_home"
    return 0
  fi

  cat >&2 <<EOF
Android builds require a full JDK with jlink.
The current Java runtime is not sufficient: ${current_java_home:-none}
Install openjdk-21-jdk or set JAVA_HOME to a full JDK.
EOF
  return 1
}

aerobag_select_android_jdk
