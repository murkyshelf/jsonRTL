#!/usr/bin/env bash
set -euo pipefail

harness_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
dls_source="${DLS_SOURCE:-/tmp/jsonrtl-dls-upstream}"
previous_arg=""
for arg in "$@"; do
  if [[ "$previous_arg" == "--upstream" ]]; then dls_source="$arg"; fi
  previous_arg="$arg"
done
if [[ -n "${DOTNET_BINARY:-}" ]]; then
  dotnet_binary="$DOTNET_BINARY"
elif command -v dotnet >/dev/null; then
  dotnet_binary="$(command -v dotnet)"
elif [[ -x /tmp/jsonrtl-dotnet/dotnet ]]; then
  dotnet_binary=/tmp/jsonrtl-dotnet/dotnet
else
  echo "Requires .NET 8 SDK. Set DOTNET_BINARY or install it outside the product." >&2
  exit 1
fi
if [[ ! -f "$dls_source/Assets/Scripts/Simulation/Simulator.cs" ]]; then
  echo "Requires upstream DLS source. Set DLS_SOURCE or pass --upstream PATH." >&2
  exit 1
fi
export DOTNET_CLI_TELEMETRY_OPTOUT=1
export DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1
export DOTNET_NOLOGO=1
export DOTNET_GENERATE_ASPNET_CERTIFICATE=false
"$dotnet_binary" build "$harness_dir/DlsHarness.csproj" -p:DlsRoot="$dls_source" --nologo --verbosity quiet >&2
exec "$dotnet_binary" "$harness_dir/bin/Debug/net8.0/DlsHarness.dll" "$@"
