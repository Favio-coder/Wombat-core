# WOMBAT node para Linux x86_64

Este paquete contiene el binario precompilado `wombat-node`, un archivo público
de ejemplo `trusted-issuers.json` y archivos de configuración de ejemplo. No
necesitas clonar el repositorio ni instalar Rust o Cargo.

## Instalar y ejecutar

1. Descarga `wombat-node-linux-x86_64.tar.gz` desde GitHub Releases y
   descomprímelo:

   ```sh
   tar -xzf wombat-node-linux-x86_64.tar.gz
   cd wombat-node-linux-x86_64
   ```

2. Verifica los archivos y revisa `relay.env.example`. Sustituye la dirección
   multiaddr del relay (`/ip4/.../tcp/9000/p2p/...`), el topic y el archivo de
   trusted issuers. El `trusted-issuers.json` incluido contiene claves públicas
   de ejemplo; reemplázalo o edítalo para confiar en los emisores apropiados.

   ```sh
   sha256sum -c SHA256SUMS
   cp relay.env.example relay.env
   # Edita relay.env con tu editor preferido.
   ```

3. Carga la configuración de shell y ejecuta el nodo:

   ```sh
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

`--identity-key` crea una identidad de transporte libp2p Ed25519 local con
permisos `0600` y conserva el PeerId entre reinicios. Mantén
`wombat-transport.key` privada y haz una copia segura si necesitas preservar la
identidad. Esta clave no firma WONBs y no reemplaza las claves públicas de
`trusted-issuers.json`.

El relay multiaddr debe proporcionarla el operador del relay. Asegúrate de
revisar topic y trusted issuers antes de conectarte; la lista incluida es solo
un ejemplo público, no una recomendación universal de confianza.
