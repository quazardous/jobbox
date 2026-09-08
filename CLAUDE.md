# jobbox — ce qu'il faut savoir AVANT d'agir

Ce fichier est court exprès. Il ne contient que ce qui se viole **en
agissant naturellement** — c'est-à-dire avant qu'on ait l'idée d'ouvrir
une doc. Le reste est dans `README.md`, `USAGE.md`, `CLI-AI.md` et
`CONTRIBUTING.md`, qui font autorité sur leur sujet.

## 1. Les tickets vont sur le projet `jobbox`

**Ne jamais passer `project:` à `ticket_new`.** `$AIBALL_PROJECT` fait
autorité et vaut `jobbox` ici ; le défaut de l'outil la reprend.

Le piège est réel : ce dépôt vit sous `BookShepherd/`, dont le
`CLAUDE.md` dit que les tickets se suivent sur les projets
`BookShepherd` et `m2m-image`. **Cette phrase parle de la flotte
BookShepherd, pas de jbx.** Le 08/09/2026 un ticket est parti au mauvais
endroit pour cette raison exacte, et david a d'abord cru à un défaut
d'aiball.

En cas de doute, `ticket_get` sur un ticket récent du même sujet répond
en une seconde.

## 2. Tout est en anglais

**Le dépôt est PUBLIC** (`quazardous/jobbox`, MIT). Code, commentaires,
messages de commit, documentation, messages d'erreur : anglais.

Ça vaut pour les commits même quand la conversation est en français —
trois commits français ont été poussés le 08/09/2026 dans un historique
entièrement anglais, et ils y restent. Lire `git log` avant d'écrire.

> **Note :** aiball classe ce projet en `private`, dont le kit autorise
> le français dans les commentaires et les références internes. C'est
> faux ici. La discipline `public` s'applique.

## 3. On ne construit pas par-dessus le binaire qu'on exécute

```bash
bin/promote      # build → test à blanc dans un HOME jetable → copie
```

`cargo build` écrit dans `target/`, et si le binaire installé pointe
dessus — lien symbolique, ou `PATH` qui y mène — un binaire à moitié
écrit devient **le crochet de la session en cours**. C'est arrivé : une
récursion infinie dans `config::path()` a gelé chaque commande, et david
a dû éditer `~/.claude/settings.json` à la main, hors de Claude Code,
pour s'en sortir.

`bin/promote` **copie**, jamais ne lie, et refuse de promouvoir un
binaire qui ne répond pas à un `config` et à une charge de crochet.

## 4. jbx enveloppe TES propres commandes

Le crochet est posé dans cette session. Une commande de premier plan qui
dépasse la coupe (30 s par défaut) **est détachée pendant que tu
l'attends** — sa sortie ne revient pas, et le résultat arrive plus tard
sous un id.

En pratique : pas de `sleep` long au premier plan, `< /dev/null` sur ce
qui pourrait lire son entrée, et `run_in_background` pour ce qui dure.
Un `cargo test` bavard suffit à déclencher le détachement.

**Et `JBX_WRAPPED` est dans l'environnement**, donc un `jbx run` lancé à
la main s'efface au lieu de détacher — c'est voulu (#2066). Pour éprouver
le détachement, `env -u JBX_WRAPPED`, sans TTY.

## 5. Le dépôt frère

`../simai-cli` (`quazardous/simai-cli`) joue les cinq clients d'agent
pour éprouver un crochet sans les installer, et c'est lui qui produit le
GIF du README. Son scénario vit **ici**, dans `demo/detach.txt`, puisque
c'est jbx qu'il démontre.

## 6. Ce qui a déjà été refusé

`CONTRIBUTING.md` § *What was already ruled out* porte la liste, et elle
vaut d'être lue avant de proposer. La première entrée est celle qui
revient le plus souvent : **prédire quelles commandes seront longues**,
construite jusqu'au banc d'essai puis abandonnée sur les mesures. Une
règle rétrospective ne peut pas attraper une commande qu'elle n'a jamais
vue, et les commandes longues sont presque toujours nouvelles.

Proposer une de ces idées est légitime ; la proposer sans savoir qu'elle
a été écartée fait perdre du temps à tout le monde.
