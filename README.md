# Forked

Client Git pour **macOS et Windows**, écrit en Rust et inspiré de
[Fork](https://git-fork.com). L'interface utilise [egui](https://github.com/emilk/egui).
La lecture du dépôt passe par libgit2 ([git2](https://crates.io/crates/git2)), et
toutes les écritures par le `git` installé sur la machine : vos hooks, la signature
des commits, vos clés SSH et vos credential helpers fonctionnent comme dans un terminal.

## macOS

### Prérequis

- macOS 11 ou plus récent
- Command Line Tools d'Apple (`xcode-select --install`). Ce n'est pas l'IDE Xcode,
  mais un petit paquet qui fournit l'éditeur de liens et le SDK macOS dont Rust a
  besoin pour produire un exécutable, `clang` pour compiler libgit2 (du C), ainsi que
  `codesign` et `iconutil` utilisés par le script d'installation. Ils sont souvent
  déjà présents : `xcode-select -p` affiche leur emplacement.
- Git (celui de Homebrew, `/opt/homebrew/bin/git`, est utilisé en priorité s'il existe)
- Rust, installé via Homebrew :

  ```sh
  brew install rustup
  rustup default stable
  # rustup n'est pas lié dans /opt/homebrew : ajoutez-le au PATH
  echo 'export PATH="/opt/homebrew/opt/rustup/bin:$PATH"' >> ~/.zshrc
  ```

### Installer l'application

```sh
./scripts/install.sh
```

Le script :

1. compile en mode release ;
2. génère l'icône (`scripts/make_icon.swift`) ;
3. crée `Forked.app` et lui applique une signature locale (aucun compte développeur requis) ;
4. la copie dans `~/Applications`, ou dans `$FORKED_INSTALL_DIR` si cette variable est définie.

Forked apparaît ensuite dans Spotlight (⌘ Espace → « Forked »), le Launchpad et
le Dock, sans aucun déploiement. Relancez le script après chaque modification
du code pour mettre l'application à jour.

## Windows

Windows 10 ou 11, avec [Git for Windows](https://git-scm.com/download/win).
Deux façons d'installer Forked :

### Option A : compiler sur le PC Windows

1. Installer les prérequis :
   - les [Visual Studio Build Tools](https://visualstudio.microsoft.com/fr/visual-cpp-build-tools/),
     avec la charge de travail « Développement Desktop en C++ ». C'est l'équivalent
     des Command Line Tools d'Apple : l'éditeur de liens, le SDK Windows, et le
     compilateur C pour libgit2. L'IDE Visual Studio n'est pas nécessaire ;
   - Rust, avec [rustup-init.exe](https://rustup.rs) ;
   - [Git for Windows](https://git-scm.com/download/win).
2. Dans PowerShell, à la racine du projet :

   ```powershell
   powershell -ExecutionPolicy Bypass -File scripts\install.ps1
   ```

### Option B : préparer l'application depuis un Mac

Pour ne rien installer sur le PC à part Git :

```sh
brew install mingw-w64              # une seule fois
./scripts/build-windows.sh
```

Le script compile pour Windows et produit `target/dist/Forked-windows.zip`
(`Forked.exe`, `forked.ico` et `install.ps1`). Copiez l'archive sur le PC,
décompressez-la, puis lancez dans le dossier obtenu :

```powershell
powershell -ExecutionPolicy Bypass -File install.ps1
```

### Ce que fait `install.ps1`

Il copie `Forked.exe` dans `%LOCALAPPDATA%\Programs\Forked` (ou dans
`$env:FORKED_INSTALL_DIR`), puis crée un raccourci dans le **menu Démarrer** et
sur le **Bureau**. L'option `-NoDesktopShortcut` désactive ce dernier. Aucun droit
administrateur n'est nécessaire. Forked se lance ensuite comme n'importe quelle
application : tapez « Forked » dans le menu Démarrer.

### Différences avec la version macOS

- Les onglets sont sous la barre de titre Windows, pas intégrés dedans.
- Les raccourcis utilisent Ctrl à la place de ⌘.
- Les polices sont Segoe UI et Cascadia Mono (ou Consolas).
- Les boutons « Finder » et « Terminal » ouvrent l'Explorateur et Windows Terminal
  (ou l'invite de commandes s'il n'est pas installé).

## Développement

```sh
cargo run                         # lance Forked
cargo run -- ~/chemin/vers/depot  # lance Forked et ouvre ce dépôt
cargo test                        # tests unitaires et d'intégration
cargo clippy --all-targets        # lint
cargo clippy --all-targets --target x86_64-pc-windows-gnu   # lint du code Windows depuis un Mac
```

Une fois installée, l'application accepte aussi un chemin :
`open -a Forked --args ~/chemin/vers/depot` sur macOS, `Forked.exe C:\chemin\vers\depot`
sur Windows.

Le code propre à chaque système (ouverture de fichiers, terminal, polices,
emplacement de `git`, raccourcis) est regroupé dans `src/platform.rs`.

### Captures d'écran automatiques

Pour vérifier le rendu sans interagir avec la fenêtre :

```sh
FORKED_SCREENSHOT=/tmp/shot.bmp FORKED_VIEW=history  cargo run --release -- ~/depot
FORKED_SCREENSHOT=/tmp/shot.bmp FORKED_VIEW=changes  cargo run --release -- ~/depot
FORKED_SCREENSHOT=/tmp/shot.bmp FORKED_VIEW=checkout:ma-branche cargo run --release -- ~/depot
sips -s format png /tmp/shot.bmp --out /tmp/shot.png
```

L'application affiche la vue demandée, enregistre la fenêtre puis se ferme.

## Fonctionnalités

### Dépôts et onglets
- Plusieurs dépôts ouverts en onglets ; ils sont restaurés au relancement.
- Ouvrir, cloner ou créer un dépôt (`git init`), depuis la page « Nouvel onglet » ou le bouton +.
- Glisser un dossier dans la fenêtre pour l'ouvrir.
- Liste des dépôts récents.
- Actualisation automatique quand la fenêtre reprend le focus.

### Historique
- Graphe coloré des branches, avec les commits de fusion distingués.
- Badges de refs : branche courante, branches locales et distantes, tags. Une branche locale synchronisée avec son remote n'affiche qu'un badge, marqué d'un nuage.
- Avatars, auteur, date et SHA en colonnes.
- Recherche par message, auteur ou SHA, et navigation au clavier ↑ ↓.
- Détail d'un commit : auteur, commiteur, parents cliquables, message, fichiers modifiés en arborescence ou en liste, et diff.
- Menu contextuel d'un commit : checkout, nouvelle branche, nouveau tag, cherry-pick, revert, fusion, rebase, reset (soft, mixed ou hard), copier le SHA ou le message.
- **Double-clic sur un badge de branche** : ouvre la fenêtre de checkout.

### Checkout
- Fenêtre « Checkout » comme dans Fork. Si vous avez des modifications locales, elle propose trois options :
  - ne rien changer ;
  - stash puis réappliquer ;
  - annuler les modifications.
- Pour une branche distante, la fenêtre crée une branche locale qui la suit, ou réutilise la branche locale existante.

### Modifications locales
- Explorateur des fichiers non indexés et indexés : dossiers repliables, dossiers à un seul enfant compactés (`src/git`), badges de statut, ou liste à plat.
- Indexer ou désindexer un fichier (double-clic), un dossier ou tout le dépôt.
- **Indexation partielle** : par bloc, ou par ligne (clic sur une ligne, ⇧-clic pour une plage). Même chose pour désindexer ou annuler. Les fichiers en fins de ligne Windows (CRLF) sont pris en charge.
- Annuler les modifications d'un fichier ou d'un dossier, toujours après confirmation.
- Ajouter un fichier non suivi au `.gitignore`.
- Message de commit en deux champs, **titre** puis **description**, avec un compteur de caractères. Option amend, et commit avec ⌘↩ (Ctrl+Entrée sur Windows).
- Conflits : choisir « ours » ou « theirs », ou marquer le fichier comme résolu.

### Branches, remotes, tags et stashes
- Barre latérale avec branches en arborescence (`feat/…`, `fix/…`) et compteurs d'avance et de retard (↑ ↓).
- Branches : checkout, fusion (par défaut, `--no-ff`, `--ff-only` ou `--squash`), rebase, push, pull, renommer, supprimer, ne plus suivre la branche distante.
- Remotes : fetch, copier l'URL, ajouter ou supprimer un remote, supprimer une branche distante.
- Tags : créer (simple ou annoté), pousser, supprimer en local ou sur le remote.
- Stashes : créer (avec ou sans les fichiers non suivis), appliquer, pop, supprimer, afficher leur contenu.
- Barre d'outils : Fetch, Pull, Push (avec le nombre de commits à tirer ou pousser), Stash, Pop, Branche, Finder ou Explorateur, Terminal.
- Push avec `--set-upstream`, `--force-with-lease` et les tags.
- Bandeau pour une opération en cours (merge, rebase, cherry-pick, revert, bisect) : continuer, passer un commit ou abandonner.

### Interface
- Polices du système (SF Pro et SF Mono sur macOS, Segoe UI et Cascadia Mono sur Windows) et icônes [Phosphor](https://phosphoricons.com).
- Thèmes clair et sombre, qui suivent le réglage du système.
- Opérations Git lancées en arrière-plan, l'une après l'autre pour un même dépôt, avec des notifications de succès ou d'erreur.

### Raccourcis clavier

| macOS | Windows | Action |
|---|---|---|
| ⌘O | Ctrl+O | Ouvrir un dépôt |
| ⌘T | Ctrl+T | Nouvel onglet |
| ⌘W | Ctrl+W | Fermer l'onglet |
| ⌃⇥ | Ctrl+Tab | Onglet suivant |
| ⌘R | Ctrl+R | Actualiser |
| ⌘1 / ⌘2 | Ctrl+1 / Ctrl+2 | Modifications locales / Tous les commits |
| ⌘↩ | Ctrl+Entrée | Commit |
| ↑ ↓ | ↑ ↓ | Naviguer dans l'historique |

## Feuille de route

Fonctionnalités de Fork qui restent à ajouter, les plus utiles d'abord :

1. **Rebase interactif** : réordonner, fusionner (squash) ou renommer des commits.
2. **Outil de résolution de conflits intégré**, en trois volets : ma version, leur version, résultat.
3. **Blame et historique d'un fichier.**
4. **Diff côte à côte**, en plus du diff unifié.
5. **Sélection multiple** de fichiers pour indexer ou annuler plusieurs fichiers d'un coup.
6. **Submodules, Git LFS et git-flow.**

## Structure du projet

```
src/
├── main.rs          Point d'entrée, fenêtre
├── app.rs           Onglets, file d'opérations Git, raccourcis, notifications
├── tab.rs           État d'un dépôt ouvert (données, sélection, message de commit)
├── state.rs         Onglets et dépôts récents enregistrés entre deux lancements
├── platform.rs      Différences entre macOS, Windows et Linux
├── debug_shot.rs    Captures d'écran automatiques
├── git/
│   ├── repo.rs      Lecture via libgit2 : log, statut, diffs, branches…
│   ├── cli.rs       Exécution du `git` du système pour les écritures
│   ├── graph.rs     Placement du graphe des commits
│   └── patch.rs     Patchs pour l'indexation par bloc ou par ligne
└── ui/
    ├── theme.rs     Couleurs et polices
    ├── widgets.rs   Lignes de liste, boutons, badges, avatars
    ├── icons.rs     Codes des icônes Phosphor
    ├── file_tree.rs Explorateur de fichiers
    ├── history.rs   Liste des commits et détail
    ├── changes.rs   Modifications locales et message de commit
    ├── diff.rs      Affichage des diffs
    ├── sidebar.rs   Barre latérale
    ├── toolbar.rs   Barre d'outils
    ├── checkout.rs  Logique du checkout
    └── dialogs.rs   Fenêtres modales
build.rs             Intègre l'icône dans Forked.exe (Windows)
scripts/
├── install.sh        macOS : compilation, bundle .app et installation
├── make_icon.swift   macOS : génération de l'icône
├── install.ps1       Windows : installation et raccourcis
└── build-windows.sh  Compilation Windows depuis un Mac et archive
assets/
├── fonts/           Police Phosphor (licence MIT, voir PHOSPHOR-LICENSE)
└── icon/            Icône pour Windows (.ico) et pour la fenêtre (.png)
```

## Licences

- Icônes : [Phosphor Icons](https://phosphoricons.com), licence MIT (`assets/fonts/PHOSPHOR-LICENSE`).
- Polices système (SF Pro, SF Mono, Segoe UI, Cascadia Mono) : lues depuis le système au lancement, elles ne sont pas incluses dans le projet.
