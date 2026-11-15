# WOMBAT node para Linux x86_64, Linux ARM64 y Windows x86_64

Este paquete contiene el binario precompilado `wombat-node`, un archivo público
de ejemplo `trusted-issuers.json` y archivos de configuración de ejemplo. No
necesitas clonar el repositorio ni instalar Rust o Cargo.

## Linux x86_64

Descarga `wombat-node-linux-x86_64.tar.gz` desde GitHub Releases y
descomprímelo:

```sh
tar -xzf wombat-node-linux-x86_64.tar.gz
cd wombat-node-linux-x86_64
```

## Raspberry Pi 5 / Debian ARM64

Descarga el paquete `wombat-node-linux-aarch64.tar.gz` del tag deseado y
extráelo. Este paquete es para Linux ARM64 (`aarch64`); el paquete
`wombat-node-linux-x86_64.tar.gz` no es compatible con Raspberry Pi.

```sh
release_tag="vX.Y.Z"
package="wombat-node-linux-aarch64.tar.gz"
curl -fL "https://github.com/Favio-coder/Wombat-core/releases/download/${release_tag}/${package}" -o "$package"
tar -xzf "$package"
cd wombat-node-linux-aarch64
sha256sum -c SHA256SUMS
cp relay.env.example relay.env
# Edita relay.env con tu editor preferido.
set -a
. ./relay.env
set +a
./wombat-node run \
  --listen "$WOMBAT_LISTEN" \
  --peer "$WOMBAT_RELAY" \
  --topic "$WOMBAT_TOPIC" \
  --trusted-issuers "$WOMBAT_TRUSTED_ISSUERS" \
  --identity-key "$WOMBAT_IDENTITY_KEY"
```

## Windows x86_64 (PowerShell)

Descarga el ZIP de la página de GitHub Releases para el tag que quieras
instalar, expándelo y entra en la carpeta:

```powershell
$releaseTag = "vX.Y.Z"
$zipPath = "$PWD\wombat-node-windows-x86_64.zip"
$releaseUrl = "https://github.com/Favio-coder/Wombat-core/releases/download/$releaseTag/wombat-node-windows-x86_64.zip"
Invoke-WebRequest -Uri $releaseUrl -OutFile $zipPath
Expand-Archive -Path $zipPath -DestinationPath ".\wombat-node-windows-x86_64" -Force
Set-Location ".\wombat-node-windows-x86_64"
```

Revisa y edita `relay.env.example`; sustituye la dirección multiaddr del relay,
el topic y los trusted issuers. El `trusted-issuers.json` incluido contiene
claves públicas de ejemplo: reemplázalo o edítalo para confiar en los emisores
apropiados. Comprueba los SHA-256 incluidos en `SHA256SUMS` antes de ejecutar.

```powershell
Get-Content .\SHA256SUMS | ForEach-Object {
  if ($_ -notmatch '^([0-9a-f]{64})  (.+)$') {
    throw "Formato inválido en SHA256SUMS: $_"
  }
  $expected = $Matches[1]
  $file = $Matches[2]
  $actual = (Get-FileHash -LiteralPath ".\$file" -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $expected) {
    throw "SHA-256 no coincide: $file"
  }
}
Copy-Item .\relay.env.example .\relay.env
# Edita .\relay.env con tu editor preferido.
```

Ejecuta el binario directamente desde PowerShell, con los valores adecuados
para tu red:

```powershell
.\wombat-node.exe run `
  --listen "/ip4/0.0.0.0/tcp/0" `
  --peer "/ip4/<RELAY_PUBLIC_IP>/tcp/9000/p2p/<RELAY_PEER_ID>" `
  --topic "wombat/pe/piura/piura-demo/change" `
  --trusted-issuers ".\trusted-issuers.json" `
  --identity-key ".\wombat-transport.key"
```

## Ejecutar en Linux

Después de descargar `wombat-node-linux-x86_64.tar.gz`, verificar sus archivos
y revisar la configuración de ejemplo:

```sh
sha256sum -c SHA256SUMS
cp relay.env.example relay.env
# Edita relay.env con tu editor preferido.
set -a
. ./relay.env
set +a
./wombat-node run \
  --listen "$WOMBAT_LISTEN" \
  --peer "$WOMBAT_RELAY" \
  --topic "$WOMBAT_TOPIC" \
  --trusted-issuers "$WOMBAT_TRUSTED_ISSUERS" \
  --identity-key "$WOMBAT_IDENTITY_KEY"
```

`--identity-key` crea una identidad de transporte libp2p Ed25519 local y
conserva el PeerId entre reinicios. En Linux el archivo se crea con permisos
`0600`. Mantén `wombat-transport.key` privada y haz una copia segura si
necesitas preservar la identidad. Esta clave no firma WONBs ni reemplaza las
claves públicas de `trusted-issuers.json`.

El relay multiaddr debe proporcionarla el operador del relay. Revisa topic y
trusted issuers antes de conectarte; la lista incluida es solo un ejemplo
público, no una recomendación universal de confianza.
