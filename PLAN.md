# HUESYNC — Plan de développement

Contrôle des ampoules Philips Hue en synchronisation avec la musique, avec un éditeur
spatial (pièce/appartement + positions 3D des lampes) et des effets spatiaux.

## Choix retenus

| Sujet | Choix |
|---|---|
| Stack | Rust (moteur temps réel) + Tauri (app desktop) + React / Three.js (UI) |
| Bridge | Hue Bridge Pro |
| Source audio | Deezer / YouTube joués sur le Mac → capture du son système |
| Plateforme | macOS (Tauri permet Windows/Linux plus tard) |

## Contraintes techniques Hue

- **API REST CLIP v2** (`https://<bridge>/clip/v2/resource/...`) : utilisée pour la
  configuration uniquement (découverte, lampes, zones). Trop lente pour le temps réel
  (~10 cmd/s pour les lampes, ~1 cmd/s pour les groupes).
- **API Cloud** : non utilisée (latence internet).
- **API Entertainment** : utilisée pour le streaming temps réel.
  - Appairage : `POST /api` avec `{"devicetype":"huesync#mac","generateclientkey":true}`
    → `username` (header `hue-application-key`) + `clientkey` (PSK DTLS).
  - Zone : ressource `entertainment_configuration` avec positions des canaux en x/y/z ∈ [-1, 1].
  - Démarrage : `PUT /clip/v2/resource/entertainment_configuration/<id>` `{"action":"start"}`,
    arrêt avec `{"action":"stop"}`.
  - Flux : DTLS 1.2 PSK, `TLS_PSK_WITH_AES_128_GCM_SHA256`, UDP port 2100.
    PSK identity = `username`, PSK = `clientkey` (hex).
  - Message `HueStream` v2 : header (`"HueStream"`, version 2.0, seq, couleur RGB ou XY),
    ID de la zone (36 caractères), puis 7 octets par canal (id + 3 × 16 bits).
  - Envoi à 50–60 Hz ; le bridge transmet ~25 Hz vers Zigbee (lissage côté bridge).
  - Bridge Pro : plus de canaux par zone que le Bridge v2 (nombre exact à vérifier).
- Événements : flux SSE `GET /eventstream/clip/v2` pour suivre l'état (zone prise par
  une autre app, lampe injoignable…).
- Certificat HTTPS du bridge : signé par la CA Signify (à épingler) ; le CN = bridge id.

> Les pages developers.meethue.com nécessitent un compte : vérifier les valeurs exactes
> (limites de canaux Bridge Pro, format du header) une fois connecté.

## Pourquoi Rust

Le goulot d'étranglement est le bridge + Zigbee (~25 Hz ≈ 40 ms/frame), pas le CPU :
FFT et calcul des couleurs prennent quelques microsecondes. Les critères décisifs sont
donc la **régularité du timing** (pas de GC ni de GIL), le **support DTLS-PSK**, la
**capture audio native** et une **UI riche**. Rust donne les performances du C sans
ses risques, et Tauri fournit une app native légère avec une UI web (Three.js pour la 3D).

## Capture audio (Deezer / YouTube)

Le son vient d'apps ou du navigateur → il faut capter la sortie audio du Mac :

1. **BlackHole** (pilote virtuel gratuit) + périphérique multi-sorties macOS : simple,
   marche tout de suite avec `cpal`. → utilisé en phase 1.
2. **Core Audio Process Taps** (macOS 14.2+) : capture d'une app précise (Chrome,
   Deezer) ou de tout le système, sans pilote. → cible finale (phase 4), via FFI
   Core Audio (crate `coreaudio-sys` / `cidre`).
3. Micro en solution de secours.

L'API d'analyse audio Spotify étant dépréciée et Deezer/YouTube n'en proposant pas,
toute l'analyse est faite localement.

## Architecture

```
┌──────────── App Tauri ───────────────────────────────────────┐
│ UI (React + TypeScript + react-three-fiber)                  │
│  - Éditeur pièce 2D/3D  - Placement lampes  - Panneau effets │
│  - Prévisualisation live (lampes virtuelles)                 │
├──────────── commandes / événements Tauri ────────────────────┤
│ Moteur Rust                                                  │
│  audio ──► analyse ──► moteur d'effets ──► sortie            │
│  (cpal)    FFT, bandes,   f(pos, t, audio)   ├─ DTLS Hue     │
│            onsets, BPM    → couleur          └─ simulateur   │
│  bridge : découverte mDNS, appairage, REST v2, SSE           │
└──────────────────────────────────────────────────────────────┘
```

Threads :
- **audio** : callback `cpal` → ring buffer lock-free (`rtrb`).
- **analyse** : FFT (`rustfft`) toutes les ~10 ms → `AudioFeatures` partagées.
- **engine** : boucle 60 Hz à cadence fixe → calcule les couleurs → sortie(s).
- **UI** : reçoit l'état des lampes à ~30 Hz pour la prévisualisation.

### Modèle de données

```text
Project
 ├─ rooms[]        polygone 2D (mètres) + hauteur sous plafond
 ├─ lights[]       { id, hue_light_id, hue_channel_id, position: (x,y,z) en mètres, nom }
 ├─ presets[]      pile de calques d'effets + paramètres
 └─ settings       bridge, zone, offset audio, luminosité max, limite strobe
```

Positions en mètres dans l'UI → normalisées en [-1, 1] pour la zone Hue ; les effets
travaillent sur les coordonnées normalisées du bounding box de l'appartement.

### Moteur d'effets

Chaque effet implémente :

```rust
trait Effect {
    fn update(&mut self, ctx: &FrameContext);           // temps, dt, audio, BPM/phase
    fn color(&self, light: &LightInfo) -> Rgba;         // position xyz normalisée
}
```

Les calques sont composés (normal, add, multiply, max) avec une opacité.

Effets prévus :
- **Couleur unie** (palette, respiration douce sur l'énergie).
- **Balayage** : plan mobile (axe x/y/z/diagonale/angle libre), largeur, dégradé ;
  1 ou 2 traits (parallèles, miroir, croisés) ; vitesse libre ou calée sur le BPM.
- **Flashs aléatoires** : base unie + N lampes aléatoires flashent sur les onsets.
- **Pulse radial** : onde sphérique depuis un point à chaque beat.
- **Par hauteur** : basses → lampes basses, aigus → lampes hautes.
- **Vague / arc-en-ciel spatial**, **chenillard** (ordre spatial).
- **Strobe** (limité en fréquence pour la sécurité photosensible).

## Phases

### Phase 0 — Validation technique (CLI Rust)
- [x] Découverte du bridge (mDNS `_hue._tcp` + fallback `https://discovery.meethue.com`).
- [x] Appairage (bouton du bridge) → `username` + `clientkey` stockés dans
      `~/Library/Application Support/huesync/bridge.json` (chmod 600 ; Keychain plus tard).
- [x] Lister lampes et `entertainment_configuration` via CLIP v2.
- [x] Démarrer le streaming + handshake DTLS-PSK (`webrtc-dtls`) + effets de test.
- **Critère de sortie** : atteint — DTLS connecté en ~26 ms, 49,9 img/s pendant 15 s,
  envoi le plus lent 8 ms (build debug).

Commandes : `cargo run -- discover | pair | info | test --area <nom> --effect blink|chase|sweep | stop`.

### Phase 1 — Moteur temps réel
- [ ] Boucle 60 Hz à cadence fixe, sortie DTLS, keep-alive, reconnexion, stop propre.
- [ ] Sortie **simulateur** (dev sans bridge).
- [ ] Capture audio `cpal` (BlackHole), ring buffer.
- [ ] Analyse : FFT, énergie par bandes (basses/médiums/aigus), spectral flux → onsets,
      estimation BPM + phase.
- [ ] 2–3 effets codés en dur pour valider la chaîne audio → lumière.

### Phase 2 — App Tauri + éditeur d'espace
- [ ] Scaffold Tauri v2 + React + TypeScript + Vite.
- [ ] Assistant de connexion au bridge (découverte, appairage, choix de la zone).
- [ ] Éditeur 2D vue de dessus : rectangle/carré, puis polygones, cotes en mètres.
- [ ] Vue 3D : placement par glisser-déposer, hauteur, association lampe Hue ↔ point
      (« identifier » = faire clignoter la lampe).
- [ ] Création/mise à jour de la zone de divertissement avec les positions.
- [ ] Sauvegarde/chargement du projet (JSON).

### Phase 3 — Effets + interface de contrôle
- [ ] Trait `Effect`, pile de calques, blend modes.
- [ ] Implémenter les effets listés ci-dessus.
- [ ] Panneau de paramètres live (couleurs, vitesse, réactivité, sync BPM).
- [ ] Prévisualisation 3D live = ce qui est envoyé aux lampes.
- [ ] Visualiseur audio (spectre, beats, BPM).

### Phase 4 — Finitions
- [ ] Capture via Core Audio Process Taps (choisir l'app : Chrome, Deezer…).
- [ ] Presets/scènes, auto-enchaînement selon l'énergie, raccourcis clavier, tap tempo.
- [ ] Offset audio → lumière, luminosité max, limite strobe.
- [ ] Multi-zones (Bridge Pro), restauration de l'état des lampes à l'arrêt.
- [ ] (Option) Télécommande mobile via serveur web local.

## Arborescence

```
HUESYNC/
├─ src-tauri/
│  ├─ src/bridge/     # découverte, REST v2, appairage, SSE, DTLS
│  ├─ src/audio/      # capture + analyse
│  ├─ src/engine/     # boucle, effets, calques
│  ├─ src/model/      # projet, pièces, lampes, presets
│  └─ src/main.rs
└─ ui/
   ├─ editor/         # éditeur 2D/3D
   ├─ effects/        # panneaux d'effets
   └─ preview/
```

## Crates / librairies envisagées

- Rust : `tauri`, `tokio`, `reqwest` (rustls, CA Signify), `mdns-sd`, `openssl`
  (DTLS PSK via `set_psk_client_callback`) ou `webrtc-dtls`, `cpal`, `rustfft`, `rtrb`,
  `serde`, `keyring`.
- UI : React, TypeScript, Vite, `three`, `@react-three/fiber`, `@react-three/drei`,
  `zustand`.

## Risques

| Risque | Mitigation |
|---|---|
| DTLS-PSK capricieux | Phase 0 dédiée ; deux crates candidates |
| Latence perçue audio → lumière | Offset réglable + analyse sur petites fenêtres |
| Capture son système macOS | BlackHole d'abord, Process Taps ensuite |
| Zone prise par Hue Sync/autre app | Surveillance SSE + message clair dans l'UI |
