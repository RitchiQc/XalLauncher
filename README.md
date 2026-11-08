# XalLauncher

Launcher Minecraft Java Edition moderne pour **Windows 10/11 x64**, développé par **Xaltar Studio** avec Tauri 2, React, TypeScript et Rust.

> XalLauncher est en développement actif. La connexion et le lancement avec un vrai compte nécessitent une application Microsoft Entra configurée. Aucun mode crack ou contournement de licence n’est proposé.

## Fonctions implémentées

### Comptes et capes

- Connexion Microsoft par **device code**, puis Xbox Live, XSTS et Minecraft Services.
- Vérification de la possession de Minecraft: Java Edition.
- Plusieurs comptes, changement de compte actif et renouvellement de session.
- Jetons conservés dans le **Coffre d’identification Windows**, jamais dans `settings.json`.
- Profil, skin avec aperçu 3D, liste des capes possédées, activation et masquage d’une cape officielle.

### Instances et lancement

- Instances isolées : mods, mondes, configurations et captures séparés.
- Versions release et snapshots du manifeste officiel Mojang.
- Vanilla, Fabric, Quilt, Forge et NeoForge.
- Détection de la version Java requise et installation automatique d’Eclipse Temurin.
- Téléchargement vérifié du client, des bibliothèques, des natives et des assets.
- Reprise des téléchargements, tentatives automatiques et vérification SHA-1.
- RAM, arguments JVM, résolution, plein écran et serveur au démarrage par instance.
- Classpaths longs via fichier d’arguments Java sur les runtimes compatibles.
- Console en direct, journal local, arrêt du processus et temps de jeu.

### Contenu

- Recherche Modrinth de mods, modpacks, resource packs et shaders.
- Filtrage par version de Minecraft et modloader.
- Installation des dépendances obligatoires et remplacement des anciennes versions d’un projet.
- Installation de la dernière publication d’un modpack Modrinth et import de fichiers `.mrpack`.
- Import, activation/désactivation et suppression de contenus locaux.
- Liste des mondes et galerie des captures d’écran.
- Sauvegardes ZIP des mondes, restauration sécurisée et sauvegarde automatique avant restauration.

### Multijoueur et application

- Serveurs favoris avec statut réel, MOTD, version, joueurs et latence.
- Association d’un serveur à une instance et lancement rapide.
- Thèmes sombre/clair, accents, animations réductibles et centre de progression.
- Recherche manuelle ou automatique de mises à jour signées via GitHub Releases.
- Interface actuellement en français.

## Démarrage du développement

### Prérequis

- Windows 10/11 x64 et WebView2.
- Node.js 24+ et npm.
- Rust stable 1.90+ via rustup.
- Visual Studio 2022 Build Tools avec la charge de travail **Développement Desktop en C++**.

```powershell
npm.cmd install
npm.cmd run tauri dev
```

Sous cette machine, PowerShell peut bloquer `npm.ps1` : utilisez donc `npm.cmd`.

### Vérifications

```powershell
npm.cmd run typecheck
npm.cmd test
npm.cmd run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets
cargo test --manifest-path src-tauri/Cargo.toml
```

Créer l’installeur et les artefacts de mise à jour :

```powershell
npm.cmd run tauri build
```


## Configuration Microsoft Entra

1. Ouvrez Microsoft Entra et créez une inscription d’application.
2. Choisissez la prise en charge des **comptes personnels Microsoft**.
3. Configurez l’application comme client public et activez le **flux de code d’appareil**.
4. Copiez l’**Application (client) ID** dans `Paramètres > Authentification Microsoft`.
5. Ajoutez ensuite un compte depuis `Comptes & capes`.

L’identifiant public de l’application XalLauncher est intégré par défaut : les joueurs n’ont pas besoin de créer leur propre application Entra. Pour utiliser une autre inscription, remplace-le dans les paramètres ou définis `XAL_MICROSOFT_CLIENT_ID` à la compilation. Aucun secret client n’est utilisé.

Pour distribuer le launcher sans demander de configuration aux joueurs, définis cette variable lors du build :

```powershell
$env:XAL_MICROSOFT_CLIENT_ID = "ton-application-client-id"
npm.cmd run tauri build
```

Pour les Releases GitHub, crée la **variable** de dépôt `XAL_MICROSOFT_CLIENT_ID` dans `Settings > Secrets and variables > Actions > Variables`. Le workflow de Release l’intégrera automatiquement comme valeur par défaut. Chaque joueur se connectera ensuite avec son propre compte Microsoft, sans créer sa propre application Entra.

## CI/CD et Releases

- `.github/workflows/ci.yml` exécute les contrôles frontend et Rust sur `main` et les pull requests.
- `.github/workflows/release.yml` compile et signe Windows sur un tag `v*`, puis publie l’installeur signé, sa signature et `latest.json`.

Secrets GitHub requis :

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Sur la machine de développement initiale, ces valeurs sont conservées hors du dépôt dans `%APPDATA%\XalLauncher\updater`. Sauvegardez-les : leur perte empêche de publier des mises à jour compatibles avec les installations existantes.

```powershell
git tag v0.1.0
git push origin v0.1.0
```

La première Release doit être publiée avant que la recherche de mise à jour puisse trouver un package.

## Structure

- `src/` : interface React, pages, composants, état et tests.
- `src-tauri/src/auth.rs` : Microsoft, Xbox, Minecraft et capes.
- `src-tauri/src/launch.rs` : préparation et processus Minecraft.
- `src-tauri/src/loaders.rs` : Fabric, Quilt, Forge et NeoForge.
- `src-tauri/src/providers/` : intégration Modrinth.
- `src-tauri/src/content.rs` : contenu local.
- `src-tauri/src/modpacks.rs` : archives `.mrpack`.
- `src-tauri/src/backup.rs` : sauvegardes des mondes.
- `src-tauri/src/servers.rs` : protocole Server List Ping.

## Limites actuelles

- Windows x64 uniquement ; macOS et Linux ne sont pas encore pris en charge.
- Les très anciennes versions de Forge peuvent nécessiter des procédures spécifiques non couvertes.
- CurseForge n’est pas intégré sans clé API et validation de ses conditions.
- Essential n’a pas d’intégration dédiée ; il reste installable comme un mod Modrinth ordinaire.
- Les capes personnalisées et les cosmétiques Essential ne sont pas modifiés par XalLauncher.
- Les tests de connexion, de possession du jeu et de lancement authentifié exigent un compte Microsoft réel ; ils ne sont pas simulés en CI.

## Mentions

XalLauncher n’est pas affilié à Mojang, Microsoft ou Modrinth. Minecraft est une marque de Mojang Synergies AB. Les contenus restent soumis aux licences de leurs auteurs et plateformes.
