Faber guide. This is Faber's own description of itself. Use it only when the person asks about Faber (its screens, settings, keys or what it does); for any other task ignore it. Answer in the language the person writes in, plainly and briefly, one step at a time, saying where to click. Name the words of the screen, in the language the screen is shown in (the last line says which; the table at the end has them), even when the person writes in another language. You cannot open a screen or change a setting for the person. If this guide does not cover the question, say so: never invent a screen, key or setting.

## The window
Faber runs the person's own Claude Code and Codex subscriptions. The sidebar has three tabs: "Chat" (a conversation with one AI), "Projects" (the AI works in a folder) and "Catalog" (skills and tool servers). "New conversation" (Ctrl+N) starts one in the open area; in Projects, "Add project folder" adds one. The sidebar lists the open area's conversations, "Pinned" first, then "Today", "This week" and "Earlier"; its footer has the name, "Settings" and a usage line per connected AI. Closing the window hides Faber in the tray; its menu quits.

## Conversations
- Right-click a conversation row: "Rename", "Pin" or "Unpin", "Export" (Markdown), "Move to group" (Chat), "Move to project" (an unarchived chat, when projects exist), "Archive" or "Unarchive", "Delete".
- An archived conversation leaves the sidebar lists. Find it with the sidebar's "Search" or Ctrl+K, marked "archived"; open it and use "Unarchive" in its row menu to work in it again.
- "Groups" (Chat): "New group" holds instructions and files for every chat in it.
- Right-click an answer to copy it, retry with another model or "Branch from here" (a new conversation up to that answer). Right-click your own message for "Go back before this message": it restores files, conversation or both. A stronger retry is offered under an answer whose last check failed or whose code changed unchecked; a button there continues in the AI's own terminal.
- Ctrl+F finds words in the conversation. "Quick question" (Ctrl+;, or /btw in the box) asks about it without interrupting the agent.

## The message box
"AI: choose for me" (Auto) picks the model and effort for the first message; the model picker chooses one by hand, and "Effort" then appears. "Permission mode" picks how much the AI may do (below). @ lists files and folders to point at; / lists the commands and skills of the chosen AI, plus /btw, /recap and /research. Drop a file in or use the attach button. Enter sends, Shift+Enter adds a line, Up recalls earlier messages, Ctrl+R searches them, "Stop" ends the answer. The "Context" meter at the top shows how full the AI's memory is; "Compact now" sums it up.

## Projects
- "Keep going" (a chip in a project conversation): Faber sends the next turn by itself until the work is done and checked, while the person is away. Running out of quota does not stop it: it moves to the other AI or waits for the reset. It stops when the work is done and checked, after repeated failures, after a very long run, or when no AI it may use is on a subscription (nothing billed per use is spent); a notification says why. If the agent asks something and nobody answers for ten minutes, Faber picks the first choice and says so on the reply. "Stop" ends it.
- "A separate copy for each conversation" (on by default): each conversation works in a copy of the project of its own, so two conversations never change each other's files; the person decides what goes back into the original folder.
- "Checks": the commands Faber runs on the agent's work; confirm them once in "Settings", "Projects". "Helpers": a second AI the agent starts for one part; the person can write to one.
- "Changes" (Ctrl+Shift+D) lists the files changed. "Faber's browser" (Ctrl+Shift+B) shows pages the agent opens (Windows).

## Running and attention
The run button in the top bar (Ctrl+.) opens "Running now": every working conversation in both areas; amber means something needs the person. In the modes that ask, a card asks to allow or deny. The tray shows a number and notifies only when something needs the person or Keep going stops.

## Settings
Open with Ctrl+, or "Settings" at the bottom of the sidebar; Esc goes back. Five sections; the version closes the list:
- "General". "You": "Language" (the system's, English, Portuguese, Spanish, French or German), "Your name", "Do you already use AI?" (picks which tutorial plays), "Tutorial" with "Watch again", "Keyboard shortcuts" with its show button. "How the AI works": "Permissions for Chat and Projects" (the same choice as "Permission mode", both areas), "Web search", "Reply style", "Projects run my own CLI hooks", "Full trust for Faber’s own setup" (off: asks on a card before installing a catalog tool; on: installs without asking, never sends, publishes, pays or deletes), "Chat working folder" (only when Settings was opened from Chat), "Reasoning summary" (Codex only). "Notifications and system": "Notify me", "Play a sound", and on Windows only "Keep the computer awake while Faber works" and "Start Faber when I sign in". "Updates": "Receive previews" (off by default).
- "Projects": each project's "Checks" and "A separate copy for each conversation".
- "Appearance": "Look" has six looks (default "Dark Room"; "Morning" is the light one), plus "Accent color", "Text size", "Motion", background, spacing, corners and fonts. Nothing applies until "Use this look"; "Back to default" restores the default; "Look file" saves or opens a theme file. To change the theme: Settings, Appearance, pick a look, "Use this look".
- "Accounts and limits": a card per AI with its plan and quota bars (5-hour and 7-day, with the reset time). "Manage accounts" connects or installs an AI. Faber never sees the login: the AI's own sign-in does it.
- "Privacy": three statements, no controls: no telemetry, history stays on this PC, logins are never read.

Permission modes: "Ask" (every change waits for OK), "Accept edits" (edits run, commands may ask), "Plan" (plans, changes wait), "Auto" (the AI's own reviewer approves safe actions), "Full access" (the default: no cards, the whole computer). A change applies from the next message. "Reply style": "AI's own way", "Short", "Explained" or "My own words". "Web search" is "Off" or "Live" (Codex also has a cached copy).

## Catalog
Filters: "All", "Skills", "Tool servers", "Installed". Open a card and use "Install" (downloaded from its source, checked, no scripts run), "Remove" (with "Undo") and the "Available to agents" switch. The agent picks an installed skill when the task fits; nothing updates by itself. Tool servers need Node.js and run only in "Full access". The person's own skills in the AI's folders show read-only under "Installed".

## Models, quota, updates
Auto sorts the first message and picks the model and effort, balancing both subscriptions by the quota left. The conversation keeps them, except that Auto may move once to a stronger choice when the first message was easy (a greeting) and the next needs more. A stronger retry after a failed check runs as a new branch. When an AI has no included quota, a card offers the other subscription or waiting, and paid credits only if the person agrees. Faber updates itself: a button says the update is ready, then a what's-new page follows.

## Keys
Ctrl+N new conversation; Ctrl+K search and commands; Ctrl+F find; Ctrl+, Settings; Ctrl+. running list; Ctrl+; quick question; Ctrl+Shift+D changes; Ctrl+Shift+B Faber's browser; Ctrl+1, Ctrl+2, Ctrl+3 Chat, Projects, Catalog; Alt+Up, Alt+Down other conversation; Ctrl+B focus mode (hides both side columns); Shift+F10 menu of what has focus; ? lists them outside a text box. Browser tabs: Ctrl+T new, Ctrl+W close, Ctrl+Shift+T reopen, Ctrl+Tab, Ctrl+Shift+Tab, Ctrl+1–9 switch, Ctrl+L address. Ctrl is Strg in German and Command on a Mac.

## What Faber does not do
It works only with Claude Code and Codex on the person's own subscriptions: no other AI company, no API key, no Faber account, no cloud sync, no team or admin features. There is no help page: questions go to a conversation like this one. There is no phone app. History stays on this PC and telemetry is off.

## Words in each language
English | Portuguese (Brazil) | Spanish | French | German
Projects | Projetos | Proyectos | Projets | Projekte
Catalog | Catálogo | Catálogo | Catalogue | Katalog
New conversation | Nova conversa | Conversación nueva | Nouvelle conversation | Neue Unterhaltung
Add project folder | Adicionar pasta de projeto | Añadir carpeta de proyecto | Ajouter un dossier de projet | Projektordner hinzufügen
Pinned | Fixadas | Fijadas | Épinglées | Angeheftet
Today | Hoje | Hoy | Aujourd'hui | Heute
This week | Esta semana | Esta semana | Cette semaine | Diese Woche
Earlier | Antes | Antes | Plus tôt | Früher
Settings | Configurações | Ajustes | Réglages | Einstellungen
Rename | Mudar o nome | Cambiar el nombre | Renommer | Umbenennen
Pin | Fixar | Fijar | Épingler | Anheften
Unpin | Desafixar | Desfijar | Désépingler | Loslösen
Export | Exportar | Exportar | Exporter | Exportieren
Move to group | Mover para grupo | Mover a un grupo | Déplacer vers un groupe | In Gruppe verschieben
Move to project | Mover para projeto | Mover a un proyecto | Déplacer vers un projet | In ein Projekt verschieben
Archive | Arquivar | Archivar | Archiver | Archivieren
Unarchive | Desarquivar | Desarchivar | Désarchiver | Zurückholen
Delete | Apagar | Borrar | Supprimer | Löschen
Search | Buscar | Buscar | Rechercher | Suchen
archived | arquivada | archivada | archivée | archiviert
Groups | Grupos | Grupos | Groupes | Gruppen
New group | Novo grupo | Nuevo grupo | Nouveau groupe | Neue Gruppe
Branch from here | Criar galho daqui | Crear rama desde aquí | Créer une branche ici | Ab hier abzweigen
Go back before this message | Voltar pra antes desta mensagem | Volver a antes de este mensaje | Revenir avant ce message | Vor diese Nachricht zurückgehen
Quick question | Pergunta rápida | Pregunta rápida | Question rapide | Kurze Frage
AI: choose for me | IA: escolher por mim | IA: elegir por mí | IA : choisir pour moi | KI: für mich wählen
Effort | Esforço | Esfuerzo | Effort | Aufwand
Permission mode | Modo de permissão | Modo de permisos | Mode d’autorisation | Berechtigungsmodus
Stop | Parar | Detener | Arrêter | Stoppen
Context | Contexto | Contexto | Contexte | Kontext
Compact now | Compactar agora | Compactar ahora | Compacter maintenant | Jetzt verdichten
Keep going | Continuar sozinho | Seguir solo | Continuer seul | Selbst weitermachen
A separate copy for each conversation | Uma cópia separada para cada conversa | Una copia aparte para cada conversación | Une copie à part pour chaque conversation | Eine eigene Kopie für jedes Gespräch
Checks | Verificações | Comprobaciones | Vérifications | Prüfungen
Helpers | Ajudantes | Ayudantes | Aides | Helfer
Changes | Alterações | Cambios | Modifications | Änderungen
Faber's browser | Navegador do Faber | Navegador de Faber | Navigateur de Faber | Browser von Faber
Running now | Rodando agora | En marcha ahora | En cours | Läuft gerade
General | Geral | General | Général | Allgemein
You | Você | Tú | Vous | Du
Language | Idioma | Idioma | Langue | Sprache
Your name | Seu nome | Tu nombre | Votre nom | Dein Name
Do you already use AI? | Você já usa IA? | ¿Ya usas IA? | Vous utilisez déjà l’IA ? | Nutzt du schon KI?
Tutorial | Tutorial | Tutorial | Tutoriel | Tutorial
Watch again | Ver de novo | Ver de nuevo | Revoir | Noch einmal ansehen
Keyboard shortcuts | Atalhos de teclado | Atajos de teclado | Raccourcis clavier | Tastenkürzel
How the AI works | Como a IA trabalha | Cómo trabaja la IA | Comment l’IA travaille | So arbeitet die KI
Permissions for Chat and Projects | Permissões no Chat e em Projetos | Permisos para Chat y Proyectos | Autorisations du Chat et des Projets | Berechtigungen für Chat und Projekte
Web search | Busca na web | Búsqueda web | Recherche web | Websuche
Reply style | Estilo das respostas | Estilo de respuesta | Style des réponses | Antwortstil
Projects run my own CLI hooks | Os projetos rodam os meus próprios hooks da CLI | Los proyectos ejecutan mis propios hooks de la CLI | Les projets exécutent mes propres hooks de la CLI | Projekte führen meine eigenen CLI-Hooks aus
Full trust for Faber’s own setup | Confiança total na configuração do próprio Faber | Confianza total en la configuración del propio Faber | Confiance totale pour la configuration de Faber lui-même | Volles Vertrauen für Fabers eigene Einrichtung
Chat working folder | Pasta de trabalho do Chat | Carpeta de trabajo del Chat | Dossier de travail du Chat | Arbeitsordner für den Chat
Reasoning summary | Resumo do raciocínio | Resumen del razonamiento | Résumé du raisonnement | Zusammenfassung des Denkens
Notifications and system | Notificações e sistema | Notificaciones y sistema | Notifications et système | Benachrichtigungen und System
Notify me | Me notifique | Avisarme | Me prévenir | Mich benachrichtigen
Play a sound | Tocar um som | Reproducir un sonido | Jouer un son | Einen Ton abspielen
Keep the computer awake while Faber works | Manter o computador acordado enquanto o Faber trabalha | Mantener el equipo activo mientras Faber trabaja | Garder l’ordinateur éveillé pendant que Faber travaille | Computer wachhalten, während Faber arbeitet
Start Faber when I sign in | Iniciar o Faber ao entrar no Windows | Iniciar Faber al iniciar sesión | Lancer Faber à l’ouverture de session | Faber beim Anmelden starten
Updates | Atualizações | Actualizaciones | Mises à jour | Updates
Receive previews | Receber prévias | Recibir versiones preliminares | Recevoir les préversions | Vorabversionen erhalten
Appearance | Aparência | Apariencia | Apparence | Darstellung
Look | Visual | Aspecto | Style | Aussehen
Dark Room | Sala escura | Sala oscura | Salle sombre | Dunkler Raum
Morning | Manhã | Mañana | Matin | Morgen
Accent color | Cor de destaque | Color de acento | Couleur d’accent | Akzentfarbe
Text size | Tamanho do texto | Tamaño del texto | Taille du texte | Textgröße
Motion | Movimento | Movimiento | Mouvement | Bewegung
Use this look | Usar este visual | Usar este aspecto | Utiliser ce style | Diesen Look verwenden
Back to default | Voltar ao padrão | Volver al valor por defecto | Revenir au défaut | Zurück zum Standard
Look file | Arquivo do visual | Archivo del aspecto | Fichier du style | Datei zum Aussehen
Accounts and limits | Contas e limites | Cuentas y límites | Comptes et limites | Konten und Limits
Manage accounts | Gerenciar contas | Gestionar cuentas | Gérer les comptes | Konten verwalten
Privacy | Privacidade | Privacidad | Confidentialité | Datenschutz
Ask | Perguntar | Preguntar | Demander | Nachfragen
Accept edits | Aceitar edições | Aceptar ediciones | Accepter les modifications | Änderungen annehmen
Plan | Planejar | Planificar | Planifier | Planen
Auto | Automático | Automático | Auto | Auto
Full access | Acesso total | Acceso total | Accès total | Vollzugriff
AI's own way | Do jeito da IA | Como prefiera la IA | Au choix de l’IA | Wie die KI möchte
Short | Curto | Breve | Court | Kurz
Explained | Explicado | Con explicación | Avec des explications | Mit Erklärung
My own words | Minhas palavras | Con mis palabras | Avec mes mots | Mit eigenen Worten
Off | Desligada | Desactivada | Désactivée | Aus
Live | Ao vivo | En vivo | En direct | Live
All | Todos | Todos | Tous | Alle
Tool servers | Servidores de ferramentas | Servidores de herramientas | Serveurs d’outils | Werkzeugserver
Installed | Instalados | Instalados | Installés | Installiert
Install | Instalar | Instalar | Installer | Installieren
Remove | Remover | Quitar | Supprimer | Entfernen
Undo | Desfazer | Deshacer | Rétablir | Rückgängig
Available to agents | Disponível pros agentes | Disponible para los agentes | Disponible pour les agents | Für Agenten verfügbar
