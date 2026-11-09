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
- Versions release et snapshots du manifeste officiel Mojang, avec source officielle de repli et nouvelle tentative en cas d’échec réseau.
- Vanilla, Fabric, Quilt, Forge et NeoForge.
- Détection de la version Java requise et installation automatique d’Eclipse Temurin.
- Téléchargements HTTPS vérifiés du client, des bibliothèques, des natives et des assets.
- Reprise des téléchargements, tentatives automatiques et vérification SHA-1, SHA-256 ou SHA-512 selon les métadonnées disponibles.
- Java Temurin vérifié par SHA-256 ; empreinte locale de `java.exe` contrôlée avant chaque lancement.
- RAM, arguments JVM, résolution, plein écran et serveur au démarrage par instance.
- Classpaths longs via fichier d’arguments Java sur les runtimes compatibles.
- Console en direct, journal local, arrêt du processus et temps de jeu.

### Contenu

- Recherche Modrinth de mods, resource packs et shaders directement depuis chaque instance.
- Recherche de modpacks depuis la Bibliothèque, avec création automatique d’une nouvelle instance isolée.
- Filtrage par version de Minecraft et modloader, avec choix de la publication exacte à installer, y compris une version plus ancienne.
- Installation des dépendances obligatoires, respect de leurs éventuelles versions imposées et remplacement des anciennes versions d’un projet.
- Identification des contenus et versions déjà installés dans l’explorateur, sans téléchargement redondant.
- Téléchargements parallèles des mods et dépendances selon le nombre de tâches configuré, avec progression globale.
- Installation de la dernière publication d’un modpack Modrinth et import de fichiers `.mrpack`.
- Import, activation/désactivation et suppression de contenus locaux.
- Avertissement explicite avant l’import de mods locaux et l’installation ou l’import de modpacks, qui contiennent du code Java exécutable.
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

## Sécurité et jetons

- Les jetons de renouvellement Microsoft et de session Minecraft sont conservés dans le Coffre d’identification Windows. Ils ne sont ni enregistrés dans les JSON du launcher ni exposés par une commande Tauri au frontend.
- Les corps des réponses d’erreur Microsoft/Xbox/Minecraft ne sont pas renvoyés à l’interface, afin d’éviter toute fuite accidentelle de données sensibles.
- Les téléchargements exigent HTTPS. Les artefacts exécutables sans empreinte dans leur profil utilisent le checksum Maven SHA-256, avec SHA-1 uniquement comme repli de compatibilité. L’absence de checksum bloque leur utilisation.
- Les publications Modrinth sont vérifiées par SHA-512 en priorité. Les fichiers en cache associés à une empreinte sont revérifiés avant réutilisation.
- Les runtimes Java installés avant l’ajout des empreintes locales seront réinstallés une fois avant leur prochain lancement.

**Limite importante :** un mod ou modpack est du code Java arbitraire exécuté dans le processus Minecraft. Le jeton de session Minecraft doit être transmis au jeu et reste donc accessible à ce code, ainsi qu’aux processus locaux capables de lire la ligne de commande Java. Un checksum garantit l’intégrité par rapport aux métadonnées de la source, pas l’innocuité de l’auteur ni la protection contre une source amont compromise. N’installez que des contenus de confiance. Le refresh token Microsoft n’est jamais transmis à Minecraft.

## CI/CD et Releases

- `.github/workflows/ci.yml` exécute les contrôles frontend et Rust sur `main` et les pull requests, ainsi que `npm audit` et `cargo audit` installé depuis un binaire Windows précompilé et vérifié.
- `.github/workflows/release.yml` compile et signe Windows sur un tag `v*`, réutilise les artefacts release des tests Rust, puis publie l’installeur signé, sa signature et `latest.json`.
- Le profil release utilise un LTO `thin` et 4 unités de génération de code pour réduire la durée des builds.
- Les actions GitHub sont épinglées par SHA de commit pour limiter les risques de supply-chain.
- Le job de publication utilise l’environnement GitHub `release`. Configure dans les paramètres du dépôt des approbateurs obligatoires et une restriction aux tags protégés avant de publier.

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
