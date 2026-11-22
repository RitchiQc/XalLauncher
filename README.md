# XalLauncher

Launcher Minecraft Java Edition moderne pour **Windows 10/11 x64**, développé par **Xaltar Studio** avec Tauri 2, React, TypeScript et Rust.

> XalLauncher est en développement actif. La connexion et le lancement avec un vrai compte nécessitent une application Microsoft Entra configurée. Aucun mode crack ou contournement de licence n’est proposé.

## Fonctions implémentées

### Comptes et capes

- Connexion Microsoft par **device code** (mode par défaut) ou par **navigateur** (Authorization Code + PKCE avec `prompt=select_account` pour choisir explicitement le compte), puis Xbox Live, XSTS et Minecraft Services.
- Aucun email ni mot de passe n’est demandé par XalLauncher : la saisie se fait uniquement sur la page officielle Microsoft. Le mode navigateur écoute la redirection sur `http://localhost:<port>` et nécessite l’URI `http://localhost` dans l’application Entra.
- Ouverture du lien de code d’appareil en **navigation privée** (Edge, Chrome, Firefox, Brave, Vivaldi, Opera) pour éviter la réutilisation silencieuse d’un compte déjà connecté dans le navigateur.
- Vérification de la possession de Minecraft: Java Edition.
- Plusieurs comptes, changement de compte actif et renouvellement de session.
- Jetons conservés dans le **Coffre d’identification Windows** (backend `windows-native` de `keyring`, secret découpé en segments pour respecter la limite de 2 560 octets par entrée), jamais dans `settings.json`.
- Profil, skin avec aperçu 3D **orienté dos par défaut** pour mettre la cape en valeur, tout en restant tournable à la souris. Liste des capes possédées, activation et masquage d’une cape officielle. Les textures officielles sont forcées en HTTPS pour que l’aperçu 3D et les capes se chargent réellement.
- Têtes de joueur dessinées directement depuis la texture officielle du skin (visage + chapeau), sans dépendre de Crafatar ou mc-heads. Le cadrage est calculé sur les dimensions réelles de la texture (64×32, 64×64 et skins HD), jamais sur une taille supposée.
- Si la texture officielle est indisponible, repli sur mc-heads puis Crafatar, et enfin sur une vignette à l’initiale du pseudo : plus jamais de carré noir.
- Vignettes de capes découpées sur la **face extérieure réelle** de la texture (colonnes 1-10, lignes 1-16), en 64×32 comme en legacy 22×17, avec mise à l’échelle calculée sur les dimensions de l’image.
- Toutes les confirmations d’actions destructives (déconnexion d’un compte, suppression ou restauration de sauvegarde, suppression de contenu ou de serveur, avertissement « code Java ») passent par la boîte de dialogue native `plugin-dialog` : `window.confirm` est remplacé par le plugin et renvoie une Promise, ce qui laissait passer les actions sans attendre la réponse.

### Instances et lancement

- Instances isolées : mods, mondes, configurations et captures séparés.
- Versions release et snapshots du manifeste officiel Mojang, avec source officielle de repli et nouvelle tentative en cas d’échec réseau.
- Vanilla, Fabric, Quilt, Forge et NeoForge, ainsi que **Horizon**, le client natif Rust de Minecraft Java Edition 26.3.
- Détection automatique de la version Java requise ou choix explicite de **Java 8, 11, 17, 21 ou 25** par instance, avec installation et vérification d’Eclipse Temurin.
- Téléchargements HTTPS vérifiés du client, des bibliothèques, des natives et des assets.
- Reprise des téléchargements, tentatives automatiques et vérification SHA-1, SHA-256 ou SHA-512 selon les métadonnées disponibles.
- Java Temurin vérifié par SHA-256 ; empreinte locale de `java.exe` contrôlée avant chaque lancement. Les runtimes sont mis en cache par version majeure dans `runtimes/` et réutilisés entre les instances.
- RAM, **version de Java** (automatique ou forcée), arguments JVM, résolution, plein écran et serveur au démarrage par instance.
- Une version Java inférieure au minimum exigé par Minecraft est refusée au lancement avec un message explicite.
- Connexion directe à un serveur au lancement : `--quickPlayMultiplayer` sur Minecraft 1.20+, avec repli sur `--server` / `--port` pour les versions antérieures.
- Image de couverture par instance : icône du projet (Modrinth ou CurseForge) pour les modpacks installés depuis le catalogue, image personnelle (PNG, JPEG ou WebP) importable depuis la page de l’instance, couverture spatiale par défaut issue du panorama XalLauncher et logo XalLauncher en repli.
- Dossier de données configurable (instances, bibliothèques, assets, runtimes, journaux, sauvegardes) avec déplacement des données existantes et redémarrage automatique.
- Classpaths longs via fichier d’arguments Java sur les runtimes compatibles.
- Console en direct, journal local, arrêt du processus et temps de jeu.
- Import d’instances Modrinth (`.mrpack`), Prism Launcher / MultiMC (`.zip` ou dossier) avec détection automatique du format, restauration des réglages Prism (RAM, serveur) et des réglages XalLauncher (`xallauncher.json`).
- Export d’une instance en `.mrpack` autonome, réimportable dans XalLauncher, Prism Launcher, MultiMC, ATLauncher et Modrinth App, avec inclusion facultative des mondes et des captures.
- L’interface indique les formats supportés dans le dialogue d’import ; les formats non reconnus produisent un message explicite.

### Horizon (moteur natif Rust)

- Instance **Horizon** dédiée à **Minecraft Java Edition 26.3**, avec builds **Nightly** récupérés depuis les Releases officielles `horizon-rust/horizon`.
- Archive Windows vérifiée par **SHA-256** contre `SHA256SUMS`, puis extraction complète (`horizon-client`, `horizon-prepare`, `research-lock.json`, `exporters/`).
- Préparation des ressources à partir de la copie Minecraft 26.3 possédée par le joueur : client, bibliothèques Maven, assets et `server.jar` téléchargé depuis le manifeste Mojang, avec vérification des empreintes.
- Le manifeste `26.3.json` est conservé **octet pour octet** (jamais resérialisé), puis son SHA-1 est comparé à `research-lock.json` ; si Mojang republie un fichier non accepté, XalLauncher utilise automatiquement le manifeste épinglé fourni par Horizon.
- **JDK 25 Temurin** dédié à `horizon-prepare` (`runtimes/jdk-25`), séparé du JRE utilisé par les instances Minecraft classiques. Aucun Java n’est utilisé pour exécuter Horizon.
- Lancement natif avec `--resources`, `--game-dir`, `--home`, résolution, plein écran et serveur de démarrage ; le jeton Minecraft est transmis par **stdin** (`--session-stdin`), jamais en ligne de commande.
- `horizon-prepare --check` est exécuté avant chaque partie ; une préparation incomplète est refaite automatiquement.
- Marché **Horizon** intégré aux onglets Mods / Mods natifs / Resource packs / Shaders d’une instance Horizon : registre `https://horizon-rust.github.io/horizon-mods/index.json`, filtrage Minecraft 26.3 et des lignes d’API servies par le build (mods 0.2, shaders 0.1 et 0.2), installation vérifiée par SHA-256 et taille.
- Mods `.hmod` installés dans `horizon-mods/` et activés dans `horizon-mods.txt` (`mod.<id>:true`) ; mods natifs `.hmod` (`kind = "native-mod"`) stockés en paquets source dans `horizon-native-mods/<id>.hmod` et activés de la même façon ; resource packs dans `resourcepacks/`, shaders `.hshader` ou `.zip` dans `shaderpacks/`.
- **Serveur officiel Horizon masqué par défaut** : XalLauncher écrit `horizon_official_server.txt` avec l’adresse `play.playhorizon.dev` avant chaque lancement, ce qui empêche Horizon d’ajouter son SMP « Horizon Official » à `servers.dat`. Les entrées déjà présentes sont retirées avec sauvegarde `servers.dat.xal-backup` ; les serveurs du joueur sont conservés. Une option dans `Paramètres > Horizon` permet de réafficher ce serveur.

### Contenu

- Recherche **Modrinth et CurseForge** de mods, resource packs et shaders directement depuis chaque instance, avec choix du fournisseur.
- Marché Horizon pour les instances Horizon : mods `.hmod`, mods natifs `.hmod` (paquets source), resource packs et shaders compatibles avec Minecraft 26.3, sans exposition des mods Java.
- Les mods natifs Horizon sont installés en **paquets source `.hmod`** et marqués « build requis » : ils ne sont pas compilés par XalLauncher et ne se chargent pas tant qu’un build natif Horizon n’a pas été produit par le launcher officiel. Les anciens `.hnmod` (fichiers ou dossiers extraits) sont détectés comme « ancien format — incompatible », ne sont jamais supprimés automatiquement et peuvent être remplacés depuis le marché ou supprimés.
- Recherche de modpacks **Modrinth, CurseForge et Horizon** depuis la Bibliothèque, avec création automatique d’une nouvelle instance isolée.
- Scroll infini dans les explorateurs (24 résultats par page, chargement automatique) et tri par pertinence, téléchargements, popularité, nouveautés ou dernière mise à jour.
- L’explorateur reste ouvert sur la page « Ajouter » après une installation, pour enchaîner plusieurs contenus sans revenir à la liste.
- Filtrage par version de Minecraft et modloader, avec choix de la publication exacte à installer, y compris une version plus ancienne.
- Installation des dépendances obligatoires CurseForge et Modrinth, respect de leurs éventuelles versions imposées et remplacement des anciennes versions d’un projet.
- Identification des contenus et versions déjà installés dans l’explorateur, sans téléchargement redondant, y compris lorsqu’un projet a été installé depuis l’autre plateforme (détection croisée par empreinte CurseForge et SHA-1 Modrinth).
- Les métadonnées des contenus installés (fournisseur, projet, version, icône) sont stockées dans `instances/<id>/metadata/` et non dans `mods`, `resourcepacks` ou `shaderpacks` : les dossiers de jeu restent propres pour les joueurs.
- Téléchargements parallèles des mods et dépendances selon le nombre de tâches configuré, avec progression globale.
- Installation de la dernière publication d’un modpack Modrinth, CurseForge ou Horizon (avec récupération de l’icône du projet), import de fichiers `.mrpack` et des archives `.zip` CurseForge ; les `.hpack` s’installent depuis la Bibliothèque via le registre Horizon.
- Import de contenus locaux **par lot** : plusieurs `.jar` ou `.zip` à la fois, ou glisser-déposer dans l’onglet Mods / Resource packs / Shaders.
- Activation/désactivation et suppression de contenus locaux.
- Avertissement explicite avant l’import de mods locaux et l’installation ou l’import de modpacks : code Java exécutable pour Minecraft, contenu `.hmod` sandboxé pour Horizon, paquet natif `.hmod` destiné à être compilé et exécuté avec les droits du compte.
- Liste des mondes et galerie des captures d’écran.
- Sauvegardes ZIP des mondes, restauration sécurisée et sauvegarde automatique avant restauration.

### Formats d’instances

| Format | Extension | Import | Export |
|---|---|---|---|
| Modrinth | `.mrpack` | ✅ | ✅ (format d’export) |
| CurseForge | `.zip` | ✅ (clé API requise si le pack contient des fichiers) | — |
| Horizon | `.hpack` | ✅ (depuis la Bibliothèque, registre Horizon) | — |
| Prism Launcher / MultiMC | `.zip` ou dossier | ✅ | — |
| XalLauncher | `.mrpack` | ✅ | ✅ |

- L’import détecte le format **par le contenu** de l’archive, pas par son extension.
- Un pack exporté par XalLauncher contient `modrinth.index.json`, tous les fichiers dans `overrides/`, `xallauncher.json` (RAM, version de Java, résolution, JVM, modules, serveur) et `xallauncher-content.json` (fournisseur, projet et version des contenus installés). Il se réimporte sans téléchargement dans XalLauncher, Prism Launcher, MultiMC, ATLauncher et Modrinth App.
- Les mondes et les captures sont exclus par défaut à l’export ; deux options permettent de les inclure.
- **Formats non pris en charge** : ATLauncher, FTB, Technic et un dossier `.minecraft` brut.
- Le pack de branding XalLauncher, les journaux, les rapports de crash et les caches sont toujours exclus de l’export.

### Modules par instance

Chaque instance possède un onglet **Modules**. Les réglages sont appliqués au lancement et ne modifient jamais l’installation d’un modloader.

- **Performance** (sans mods, désactivé par défaut) :
  - arguments JVM Aikar (G1GC, `Xms = Xmx`, `AlwaysPreTouch`, `PerfDisableSharedMem`…) ;
  - presets graphiques dans `options.txt` (distance de rendu/simulation, particules, ombres, nuages, V-Sync, FPS max…), avec sauvegarde puis restauration automatique ;
  - priorité Windows élevée et désactivation d’EcoQoS ;
  - affinité sur les cœurs performance des CPU hybrides, avec `ActiveProcessorCount` aligné.
- **Branding XalLauncher** (activé par défaut) : resource pack avec panorama cubemap 2048×2048 **pré-calculé et embarqué dans le binaire** (nébuleuse volumétrique, poussière en absorption, étoiles avec profondeur, dithering anti-banding), splashs, icône de pack et titre de fenêtre XalLauncher. Aucun rendu n'a lieu au lancement : le pack est simplement copié dans l'instance. Désactivable par instance ; le pack est alors retiré des options du jeu.
- **Diagnostic de crash** (activé par défaut) : après un arrêt anormal, analyse du journal et des rapports de crash, détection des causes fréquentes (mémoire, Java, dépendances, Mixin, graphismes, installation corrompue, port) et actions recommandées.
- **Sauvegarde automatique** : ZIP des mondes avant chaque lancement.
- **Mode jeu** : les animations lourdes du launcher (nébuleuse, étoiles, flous) sont coupées pendant une partie.

### Multijoueur et application

- Serveurs favoris avec statut réel, MOTD, version, joueurs et latence.
- Association d’un serveur à une instance et lancement rapide, avec connexion directe via Quick Play sur Minecraft 1.20+.
- Thèmes sombre, clair et **Espace** (nébuleuses animées et étoiles scintillantes), accents, animations réductibles et centre de progression.
- Recherche manuelle ou automatique de mises à jour signées via GitHub Releases (le dépôt ou l’hébergement du manifeste doit être accessible publiquement). L’installation n’est lancée qu’après une confirmation native ; sous Windows, le launcher se ferme volontairement lorsque l’installeur démarre, puis redémarre à la fin.
- Interface actuellement en français.

## Stockage et emplacement des données

- Le dossier choisi dans l’installeur NSIS ne concerne que les fichiers du programme (`XalLauncher.exe`, DLL et ressources). Les données du jeu sont stockées séparément.
- Les nouvelles installations utilisent par défaut `<dossier d’installation>\data` lorsque ce dossier est inscriptible ; sinon, `%LOCALAPPDATA%\Xaltar\XalLauncher\data` reste utilisé.
- Les installations existantes conservent leur dossier AppData. Au premier démarrage après mise à jour, XalLauncher propose de déplacer les données vers le dossier d’installation.
- Le chemin choisi est enregistré dans `%APPDATA%\Xaltar\XalLauncher\config\storage.json`.
- **Paramètres → Stockage** permet d’ouvrir le dossier, de le changer et de reprendre les instances, mods, bibliothèques, assets, runtimes, journaux et sauvegardes. Les chemins absolus des sauvegardes sont réécrits pendant le transfert ; l’ancien dossier est conservé par sécurité, notamment lors d’une copie entre disques.
- Le changement d’emplacement redémarre XalLauncher. Un jeu en cours bloque l’opération.
- Les captures d’écran et les images de couverture locales restent affichables quel que soit le dossier choisi : le protocole `asset` autorise dynamiquement le dossier de données au démarrage et la CSP autorise `asset:`, `http://asset.localhost` (Windows) ainsi que `https://asset.localhost` dans `img-src`.

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

Régénérer le panorama de branding (hors-ligne, ~45 s en release) :

```powershell
cargo run --release --manifest-path src-tauri/Cargo.toml --example bake_branding -- --size 2048 --ss 2
```

Créer l’installeur et les artefacts de mise à jour :

```powershell
npm.cmd run tauri build
```


## Configuration Microsoft Entra

Les joueurs n’ont aucune configuration à effectuer : l’identifiant public de l’application XalLauncher est intégré au launcher. Les étapes suivantes concernent uniquement les développeurs qui souhaitent utiliser leur propre inscription Microsoft Entra.

1. Ouvrez Microsoft Entra et créez une inscription d’application.
2. Choisissez la prise en charge des **comptes personnels Microsoft**.
3. Configurez l’application comme client public et activez le **flux de code d’appareil**.
4. Pour le **mode navigateur**, ouvrez `Authentification > Ajouter une plateforme > Applications mobiles et de bureau`, puis ajoutez l’URI de redirection **`http://localhost`** (sans port ni chemin) et enregistrez. Le port est ignoré par Microsoft pour les URI `localhost`.
5. Pour utiliser cette inscription, définissez son **Application (client) ID** dans `XAL_MICROSOFT_CLIENT_ID` au moment du build (voir ci-dessous).
6. Ajoutez ensuite un compte depuis `Comptes & capes`.

Le mode navigateur ouvre directement la page Microsoft avec `prompt=select_account` : le sélecteur de comptes s’affiche toujours, y compris l’option « Utiliser un autre compte ». Il est proposé via le bouton **« Utiliser un autre compte Microsoft »** dans la fenêtre de connexion ; le code d’appareil reste le mode par défaut.

L’identifiant public de l’application XalLauncher est intégré par défaut : les joueurs n’ont pas besoin de créer leur propre application Entra ni de le configurer dans le launcher. Pour utiliser une autre inscription, définis `XAL_MICROSOFT_CLIENT_ID` à la compilation. Cet identifiant est **public** : ce n’est pas un secret et il ne faut jamais utiliser de secret client Microsoft dans une application desktop.

Pour distribuer le launcher sans demander de configuration aux joueurs, définis cette variable lors du build :

```powershell
$env:XAL_MICROSOFT_CLIENT_ID = "ton-application-client-id"
npm.cmd run tauri build
```

Pour les Releases GitHub, crée la **variable** de dépôt `XAL_MICROSOFT_CLIENT_ID` dans `Settings > Secrets and variables > Actions > Variables`. Le workflow de Release l’intégrera automatiquement comme valeur par défaut. Chaque joueur se connectera ensuite avec son propre compte Microsoft, sans créer sa propre application Entra.

## Configuration CurseForge

XalLauncher prend en charge le catalogue CurseForge pour les mods, resource packs, shaders et modpacks. Les **Releases officielles intègrent une clé CurseForge dédiée** au moment du build, via le secret GitHub Actions `XAL_CURSEFORGE_API_KEY` : les joueurs n’ont donc **rien à configurer** et aucune clé n’est affichée dans l’interface.

Pour un self-build ou une version de développement sans clé intégrée, deux solutions :

1. Définir `XAL_CURSEFORGE_API_KEY` dans le terminal avant `npm run tauri build` ;
2. ou ouvrir `Paramètres > CurseForge`, coller une clé personnelle (disponible sur `https://console.curseforge.com/?#/api-keys`) et cliquer sur **Enregistrer la clé**. Elle est alors conservée dans le **Coffre d’identification Windows**, jamais renvoyée à l’interface et jamais écrite dans `settings.json`.

La clé intégrée est compilée dans le binaire : elle est invisible dans l’interface, mais un utilisateur avancé peut techniquement l’extraire. Il faut donc une clé dédiée à XalLauncher, surveiller le quota et pouvoir la remplacer sans modifier le code. Les conditions de CurseForge imposent une clé non transférable : vérifiez auprès de CurseForge/Overwolf que votre clé est autorisée pour une distribution publique. Le modèle « clé du projet intégrée aux builds officiels, override possible pour les self-builds » est celui utilisé par plusieurs launchers open source.

Les réponses de l’API ne sont pas mises en cache de façon persistante : les métadonnées minimales des contenus installés (fournisseur, projet, version, icône) sont conservées dans `instances/<id>/metadata/`, hors du dossier de jeu. Les empreintes CurseForge (MurmurHash2) et SHA-1 servent uniquement à reconnaître un contenu déjà installé, y compris depuis l’autre plateforme.

## Dépannage de la connexion Microsoft

### Le navigateur réutilise le mauvais compte (code d’appareil)

Microsoft peut ignorer le sélecteur de comptes et réutiliser la session déjà ouverte dans le navigateur. Trois solutions :

1. Cliquer sur **« Utiliser un autre compte Microsoft »** dans la fenêtre de connexion : la page Microsoft s’ouvre avec `prompt=select_account` et propose tous les comptes ainsi que « Utiliser un autre compte ».
2. Cliquer sur **« Navigation privée »** à côté du code d’appareil : le lien s’ouvre sans session Microsoft existante, ce qui force la saisie du bon email.
3. Se déconnecter au préalable sur `https://login.live.com/logout.srf`, puis relancer la connexion.

Si le message « Ce compte Microsoft ne possède pas Minecraft: Java Edition » apparaît, c’est que le compte sélectionné ne possède pas le jeu : utiliser le mode navigateur pour choisir l’email qui possède Minecraft.

### Erreur AADSTS50011 dans le navigateur

L’URI de redirection n’est pas reconnue par l’application Entra. Vérifier que **`http://localhost`** est bien enregistrée dans `Authentification > Applications mobiles et de bureau`, sans port ni chemin, et que la modification a été enregistrée. Attendre quelques minutes, puis réessayer. En attendant, le code d’appareil reste utilisable.

### Aucun email ni mot de passe dans le launcher

Microsoft ne permet pas la connexion par mot de passe direct pour les comptes personnels (ROPC interdit pour les comptes MSA, incompatible avec la double authentification et les passkeys). XalLauncher ne collecte donc jamais ces informations : toute la saisie se fait sur la page officielle Microsoft.

## Sécurité et jetons

- Les jetons de renouvellement Microsoft et de session Minecraft sont conservés dans le Coffre d’identification Windows, en plusieurs entrées (`<uuid>:meta`, `<uuid>:0`, `<uuid>:1`, …) afin de rester sous la limite de 2 560 octets du coffre. Ils ne sont ni enregistrés dans les JSON du launcher ni exposés par une commande Tauri au frontend.
- Les corps des réponses d’erreur Microsoft/Xbox/Minecraft ne sont pas renvoyés à l’interface, afin d’éviter toute fuite accidentelle de données sensibles.
- Les téléchargements exigent HTTPS. Les artefacts exécutables sans empreinte dans leur profil utilisent le checksum Maven SHA-256, avec SHA-1 uniquement comme repli de compatibilité. L’absence de checksum bloque leur utilisation.
- Les publications Modrinth sont vérifiées par SHA-512 en priorité. Les fichiers en cache associés à une empreinte sont revérifiés avant réutilisation.
- Les archives Horizon sont vérifiées par SHA-256, les fichiers du registre Horizon par SHA-256 et taille, avec une liste blanche d’hôtes de téléchargement. Les ressources préparées sont contrôlées par `horizon-prepare --check`.
- Le JDK Temurin 25 est vérifié par SHA-256 avant utilisation ; il est réservé à la préparation Horizon.
- La session Horizon est écrite sur l’entrée standard du processus puis fermée : le jeton Minecraft n’apparaît pas dans la ligne de commande.
- Chaque Release XalLauncher publie `SHA256SUMS` couvrant l’installeur et `latest.json`, avec `SHA256SUMS.sig` signé par la clé minisign Tauri. L’updater vérifie la signature du paquet avant toute installation.
- Les runtimes Java installés avant l’ajout des empreintes locales seront réinstallés une fois avant leur prochain lancement.

**Limite importante :** un mod ou modpack est du code Java arbitraire exécuté dans le processus Minecraft. Le jeton de session Minecraft doit être transmis au jeu et reste donc accessible à ce code, ainsi qu’aux processus locaux capables de lire la ligne de commande Java. Un checksum garantit l’intégrité par rapport aux métadonnées de la source, pas l’innocuité de l’auteur ni la protection contre une source amont compromise. N’installez que des contenus de confiance. Le refresh token Microsoft n’est jamais transmis à Minecraft.

## CI/CD et Releases

- `.github/workflows/ci.yml` exécute les contrôles frontend et Rust sur `main` et les pull requests, ainsi que `npm audit` et `cargo audit` installé depuis un binaire Windows précompilé et vérifié.
- `.github/workflows/release.yml` compile et signe Windows sur un tag `v*`, aligne automatiquement `package.json` et `src-tauri/Cargo.toml` sur la version du tag (elle-même lue par `tauri.conf.json` via `"version": "../package.json"`), réutilise les artefacts release des tests Rust, puis publie l’installeur signé, sa signature, `latest.json`, `SHA256SUMS` et `SHA256SUMS.sig`.
- `SHA256SUMS` couvre l’installeur renommé et `latest.json` ; il est signé avec la même clé minisign Tauri que l’updater. Une Release échoue si le manifeste ou sa signature est absent.
- Le profil release utilise un LTO `thin` et 4 unités de génération de code pour réduire la durée des builds.
- Les actions GitHub sont épinglées par SHA de commit pour limiter les risques de supply-chain.
- Le job de publication utilise l’environnement GitHub `release`. Configure dans les paramètres du dépôt des approbateurs obligatoires et une restriction aux tags protégés avant de publier.

Secrets GitHub requis :

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

Sur la machine de développement initiale, ces valeurs sont conservées hors du dépôt dans `%APPDATA%\XalLauncher\updater`. Sauvegardez-les : leur perte empêche de publier des mises à jour compatibles avec les installations existantes.

```powershell
git tag v1.0.3
git push origin v1.0.3
```

Le tag est la source de vérité : le workflow réécrit `package.json` (et son verrou), `src-tauri/Cargo.toml` et donc la version lue par Tauri avant de compiler. Il vérifie ensuite que l’installeur produit s’appelle exactement `XalLauncher_<version>_x64-setup.exe` et que sa signature porte `version:<version>`, sinon la Release échoue au lieu de publier un binaire mal versionné. Une Release doit rester **publique, publiée et non marquée pre-release** : l’updater ne s’authentifie pas et `/releases/latest/download/latest.json` renvoie 404 sur un dépôt privé, ce qui produit l’erreur « Could not fetch a valid release JSON from the remote ». La première Release doit être publiée avant que la recherche de mise à jour puisse trouver un package.

## Structure

- `src/` : interface React, pages, composants, état et tests.
- `src-tauri/src/auth.rs` : Microsoft, Xbox, Minecraft et capes.
- `src-tauri/src/launch.rs` : préparation et processus Minecraft.
- `src-tauri/src/horizon.rs` : builds Nightly, préparation, JDK 25 et lancement natif Horizon.
- `src-tauri/src/horizon_servers.rs` : masquage du SMP officiel Horizon (`horizon_official_server.txt`) et nettoyage ciblé de `servers.dat` avec sauvegarde.
- `src-tauri/src/loaders.rs` : Fabric, Quilt, Forge et NeoForge.
- `src-tauri/src/providers/` : intégration Modrinth, catalogue CurseForge et registre Horizon (mods `.hmod`, paquets natifs `.hmod`, shaders `.hshader`, modpacks `.hpack`), avec détection croisée par empreintes (`cross.rs`, `fingerprint.rs`).
- `src-tauri/src/providers/curseforge/pack.rs` : import des modpacks CurseForge (`.zip` et catalogue) et overrides.
- `src-tauri/src/secrets.rs` : clé API CurseForge dans le Coffre d’identification Windows.
- `src-tauri/src/content.rs` : contenu local.
- `src-tauri/src/content_meta.rs` : métadonnées des contenus installés, stockées hors du dossier de jeu et migrées depuis les anciens sidecars.
- `src-tauri/src/modpacks.rs` : archives `.mrpack`.
- `src-tauri/src/backup.rs` : sauvegardes des mondes.
- `src-tauri/src/servers.rs` : protocole Server List Ping.
- `src-tauri/src/storage.rs` : dossier de données, configuration et migration.
- `src-tauri/src/modules.rs` : module Performance (JVM, `options.txt`, priorité/EcoQoS, affinité P-cores).
- `src-tauri/src/branding.rs` : écriture du resource pack XalLauncher à partir du panorama pré-calculé.
- `src-tauri/examples/bake_branding.rs` : rendu hors-ligne du panorama (volume 256³, raymarching, poussière, bloom, dithering) vers `src-tauri/assets/branding/`.
- `src-tauri/src/options.rs` : lecture/écriture sûre d’`options.txt`.
- `src-tauri/src/crash.rs` : diagnostic des plantages.

## Limites actuelles

- Windows x64 uniquement ; macOS et Linux ne sont pas encore pris en charge.
- Les très anciennes versions de Forge peuvent nécessiter des procédures spécifiques non couvertes.
- CurseForge est activé dans les Releases officielles par une clé intégrée au build. Les self-builds sans secret doivent fournir une clé dans les paramètres ou via `XAL_CURSEFORGE_API_KEY`. La clé est extractible du binaire et son quota est partagé par tous les utilisateurs ; utilisez une clé dédiée et prévoyez sa rotation.
- L’utilisation d’une clé intégrée pour une distribution publique doit être confirmée avec CurseForge/Overwolf ; XalLauncher ne peut pas garantir l’autorisation contractuelle de ce modèle.
- Les mods dont l’auteur interdit la distribution via l’API CurseForge, ainsi que les modpacks contenant un fichier requis dans ce cas, ne peuvent pas être téléchargés ; XalLauncher affiche alors une erreur explicite.
- La recherche et les dépendances CurseForge ne sont pas mises à jour automatiquement ; les empreintes servent à identifier les contenus déjà installés et à éviter les doublons inter-plateformes. La détection vers CurseForge nécessite la clé API CurseForge. Si un auteur publie des fichiers binairement différents sur Modrinth et CurseForge, la correspondance par empreinte peut échouer. Aucun monde CurseForge ni cache persistant des réponses API n’est géré.
- Le canal de mise à jour lit `latest.json` sans authentification : le dépôt (ou l’hébergement du manifeste) doit être public, sinon la vérification échoue.
- Essential n’a pas d’intégration dédiée ; il reste installable comme un mod Modrinth ordinaire.
- Les capes personnalisées et les cosmétiques Essential ne sont pas modifiés par XalLauncher.
- Les images de couverture d’instance (personnalisées ou récupérées depuis Modrinth) restent locales et ne sont pas incluses dans les exports `.mrpack`.
- Le module Performance n’installe aucun mod : il agit uniquement sur la JVM, `options.txt` et le processus Windows.
- Le choix manuel d’une version Java récente (21 ou 25) n’est pas garanti avec les anciennes versions de Minecraft ou les anciens installateurs Forge ; le mode **Automatique** reste recommandé.
- Le Branding est un resource pack (menu, splashs, icône, titre) ; l’écran de chargement et l’icône de fenêtre 100 % personnalisés nécessiteraient un mod compagnon, non inclus.
- Le panorama de branding pèse environ 26 Mo de PNG embarqués (~4 à 5 Mo par face), et chaque instance reçoit sa propre copie du pack. Modifier le panorama impose de relancer `bake_branding` puis de reconstruire l’application.
- Discord Rich Presence et la mise à jour automatique des mods ne sont pas implémentés.
- Les tests de connexion, de possession du jeu et de lancement authentifié exigent un compte Microsoft réel ; ils ne sont pas simulés en CI.
- **Horizon** ne prend en charge que Minecraft Java Edition 26.3. Les mods Java Fabric/Forge/NeoForge/Quilt sont incompatibles ; le marché Horizon ne les propose pas.
- Les **mods natifs Horizon** (`native-mod`) sont installés en paquets source `.hmod` dans `horizon-native-mods/<id>.hmod` et activés dans `horizon-mods.txt`, mais XalLauncher **ne compile pas** ces mods : ils restent inactifs tant qu’un build natif Horizon n’a pas été produit par le launcher officiel. Les versions marquées `broken_on` sont masquées de l’explorateur ; celles incluses par un modpack sont installées avec l’avertissement « build requis ». Le manifeste `horizon-mod.toml` est validé (id, version, API 0.2, `native.game`, pins `horizon`/`mixins`). Les anciens `.hnmod` et dossiers extraits restent listés comme « ancien format — incompatible » : ils ne se chargent plus avec Horizon actuel, ne sont pas supprimés automatiquement et peuvent être remplacés ou supprimés depuis l’onglet Mods natifs.
- Les **modpacks Horizon** (`.hpack`) sont importés dans une nouvelle instance Horizon : le manifeste `horizon-modpack.toml` est validé, chaque mod est téléchargé et vérifié depuis le registre, les overrides autorisés sont copiés. Les mods natifs du pack sont installés en `.hmod` mais nécessiteront un build natif pour se charger ; les anciens packs qui référencent des projets archivés (comme `Atomic Industry`) ne sont plus proposés par le registre live.
- Le tag `nightly` est mobile et les tags datés sont supprimés après quelques jours ; XalLauncher met en cache chaque build par empreinte SHA-256. La première préparation prend plusieurs minutes et télécharge aussi un JDK 25.
- L’onglet **Modules** (Performance, Branding et sauvegarde automatique) est masqué pour Horizon : le client est natif et ces réglages reposent sur la JVM, `options.txt` ou le pipeline Java. Les sauvegardes manuelles des mondes restent disponibles.
- La signature minisign des projets du registre Horizon et la signature Authenticode de l’exécutable Windows ne sont pas encore vérifiées. L’updater Tauri et le manifeste `SHA256SUMS` des Releases sont signés par minisign.
- Les imports manuels de `.hmod` peuvent nécessiter une activation dans `Options > Mods` si l’identifiant interne du mod diffère du nom de fichier.

## Mentions

XalLauncher n’est pas affilié à Mojang, Microsoft, Modrinth, CurseForge ou Horizon. Minecraft est une marque de Mojang Synergies AB. CurseForge et Overwolf sont des marques de leurs propriétaires respectifs ; les données et téléchargements CurseForge sont fournis par son API et soumis à ses conditions d’utilisation. Horizon est un projet open source non officiel ; ses binaires sont téléchargés depuis ses Releases GitHub et restent soumis à leurs licences (LGPL-3.0 pour le projet Horizon, licences propres à ses dépendances). Les contenus restent soumis aux licences de leurs auteurs et plateformes.
