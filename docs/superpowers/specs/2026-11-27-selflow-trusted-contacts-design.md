# Selflow: trusted contacts over WhatsApp

- **Status:** approved by the owner on 2026-11-27
- **Created:** 2026-11-27, with the answers to the builder check (one WhatsApp session per user in
  section 5.8, the window bounding a contact's lock in section 5.1, shorten or postpone in section
  5.6, the gateway's storage in section 7, the number mode switch in section 8)
- **Phase:** 4 (Reach, main design section 11). It needs the server (M3) and the deploy on the Pi
  (M4).
- **Relates to:** the main design (`2026-11-17-selflow-design.md`), sections 6.5 (enforcement
  follows the user), 6.6 (server and transport), 8.1 (conditions), 8.3 (steps), 8.5 (unlock
  challenges), 8.9 (changing the configuration) and 8.12 (Claude Code sessions).

Decisions marked "Claude's call" were settled by Claude while writing this document and are open
to the owner's veto. Everything else was decided by the owner during the design conversation.

## 1. Why this exists

Right after work, the owner often stays locked onto something on the computer and keeps putting off
lunch. Someone in the family can see it happening but has no way to stop it.

With this feature the user picks people they trust, and those people can set off Selflow rules by
sending ordinary WhatsApp messages. The owner's two examples:

1. Lunch. A mother sends her son "vem almoçar filho". A rule recognizes the message and locks the
   screen, and the son goes to lunch because the computer is locked. The mother doesn't need to
   know a bot exists.
2. An appointment. A trusted person needs to remind the user of something important while the user
   is buried in a program. The message reaches the user as a notice or through the AI agent the
   user is working in (main design section 8.12).

The same mechanism covers the opposite case, where the user asks the trusted person to lock the
screen on purpose. Then the trusted person gets a reply such as "Locked for 30 minutes".

## 2. Goals

- Let the user name trusted contacts whose WhatsApp messages can set off rules, but only inside
  time windows the user sets for each contact.
- Work silently, so the trusted contact never learns a bot exists, or with explicit replies. The
  user picks per rule.
- Let a trusted contact act on the locks their own messages caused: grant more time, and fully
  unlock when the user allows it.
- Let the user ask for more time from the lock screen or from the phone.
- Keep only the data the feature needs (main design section 2).

## 3. Non-goals

- This isn't a plugin. The phase 4 plugins are condition sources (main design section 11), and the
  owner will widen them when that work starts. Trusted contacts are part of the server.
- An AI deciding whether a message matches a rule. That waits for the AI help idea in phase 7.
- Messengers other than WhatsApp. The provider sits behind an adapter (section 6), so another one
  can come later. (Claude's call)

## 4. Anti-goals

These would count as failure even if the feature technically works:

- Storing the text of messages, or anything at all from people who aren't trusted contacts.
- A lock that outlasts the contact's window, or that only ends if the network, the gateway and the
  trusted contact all cooperate.
- A trusted contact with power over rules they didn't set off, such as the bedtime lock.
- An easy bypass: removing the trusted contact or weakening their powers on an impulse.

## 5. How it works

### 5.1 Trusted contacts

The configuration lists the user's trusted contacts. Each one has:

- a name the user picks, used in logs, the report and the lock screen ("Locked by Mom");
- a phone number;
- time windows when their messages can set off rules, for example weekdays 11:30 to 14:00.

Outside a contact's windows, Selflow refuses every message from them (section 5.4). A window also
bounds what the contact starts inside it: a lock a contact starts ends when the window closes at
the latest, whatever length the rule gives it (owner). So the contact's replies about their own
lock always arrive inside the window.

### 5.2 A message as a rule condition

A rule can use a new condition kind next to the ones in main design section 8.1: a message from a
given trusted contact that matches a pattern. The rest of the rule works as usual, so the message
can lead to any mix of steps (main design section 8.3), such as a notice, a lock or a message to
the AI agent.

A pattern is one of:

1. A preset, ready to use (section 5.3).
2. A list of words or phrases. Matching ignores letter case and accents, so a list that holds
   "almoço" and "almoçar" matches "VEM ALMOCAR" and "o almoço tá pronto, vem". (Claude's call)
3. A regular expression, for users who want full control.

A step that messages the AI agent can carry the text of the trusted contact's message, as in "Your
mother sent: vem almoçar filho". The text goes straight to the agent and Selflow doesn't keep it
(section 7). Agent steps go through the adapters in main design section 8.12, so this design only
says "send this to the agent" and doesn't care which agent it is.

### 5.3 Presets

Selflow ships ready patterns for the replies trusted contacts use most, in English and pt-BR (main
design section 16):

- grant: "pode", "pode sim", "tá bom" and their English versions;
- grant with a length: "mais 20 minutos", "+20", where the number becomes the length
  (section 5.6);
- deny: "não", "nao";
- unlock: "libera", "pode sair";
- lock: a generic "bloqueia", for when the user asked to be locked.

The exact lists are left to the builder. Word lists and regular expressions stay available on top of
the presets.

### 5.4 Refusals outside the windows

A message from a trusted contact outside their windows sets off nothing. With replies on (section
5.5), the contact gets a short note saying it wasn't accepted at this time. With replies off,
Selflow sends nothing back.

### 5.5 Replies to the trusted contact

Each rule says whether Selflow replies to the trusted contact. Replies are off by default, so the
silent "vem almoçar" case needs no setup. (Claude's call)

- With replies on, the contact learns what happened: "Locked for 30 minutes", "Granted 20 more
  minutes", "Unlocked", or the refusal from section 5.4.
- In own-number mode (section 5.8), every reply starts with `[Selflow]`. It leaves from the user's
  own number, and without the prefix it would read as if the user wrote it.
- Reply texts can be the user's own, with defaults in both languages, like the other messages in
  main design section 8.3.

### 5.6 The trusted contact's powers over their own locks

A lock started by a trusted contact's message has a length like any other lock: a number of
minutes, until a given time, or the rest of a time window (main design section 8.3). The reply
"Locked for 30 minutes" reports that length.

The lock belongs to the contact who started it. Over that lock, and only that lock, the contact
can:

- grant as much time as they want. "mais 20 minutos" grants 20 minutes, and a plain grant such as
  "pode" grants one extension of the length the rule already defines for extensions (main design
  section 8.5). That last point is Claude's call;
- grant time without spending the rule's extension budget (main design section 8.5, challenge 4),
  because the budget limits the user and the contact is the one in charge of this lock;
- fully unlock it, when the rule allows that.

Any other lock ignores them, whether it's the bedtime lock or a lock another contact started.

Each rule picks what a grant does to the lock's end (owner):

1. Shorten. The end stays where it was, and the granted time comes out of the lock. A 30-minute
   lunch lock from 12:00 with 20 minutes granted at 12:02 comes back at 12:22 and ends at 12:30.
   Extensions in the main design work this way today.
2. Postpone. The end moves later by the granted time, so the lock keeps its full length and only
   happens later. In the same example the lock comes back at 12:22 and ends at 12:50.

Either way the lock still ends when the contact's window closes at the latest (section 5.1). When
the user picks neither, the rule shortens (owner).

Some people want to be able to leave on their own, and others want only the family to let them
out. So each rule has two switches for ending the lock early (Claude's call, in that shape):

1. whether the contact can fully unlock it ("libera");
2. whether the user can end it through the rule's unlock challenges (main design section 8.5),
   from the screen or from the terminal (main design section 8.13).

With both switches off, the lock ends when its own length runs out or when the contact's window
closes, whichever comes first (section 5.1), apart from the contact's grants. The daemon knows both
times, so the lock ends on time even while the device is offline.

When the user can't end the lock alone (switch 2 off), the challenges, `selflow unlock` and a
same-day exception (main design section 8.9) aren't offered for that lock, and the lock screen says
who can let the user out. An exception scheduled at least 24 hours ahead still skips the rule, and an
inhibit (main design section 8.4) still delays the lock, because both were set up calmly beforehand.
(Claude's call)

### 5.7 Asking for more time

The user can do it either way:

1. From the lock screen. The user writes a request there, and Selflow sends it on WhatsApp to the
   contact who started the lock.
2. From the phone. The user writes to the contact as they always do.

In both cases the contact's reply decides. While a lock they started is active, Selflow checks each
of their messages against the grant, deny and unlock patterns of that rule (section 5.3). (Claude's
call)

In own-number mode, the messages the user sends from their own account never set anything off.
(Claude's call)

### 5.8 Which number the bot uses

The user picks one of two modes:

1. Own number, the default. The gateway is linked to the user's own WhatsApp account. Trusted
   contacts keep writing to the user as they always have, which is what makes the silent case
   possible. The gateway sees every conversation, so Selflow drops everything that doesn't come
   from a trusted contact (section 7).
2. Separate number. The gateway has a number of its own, and trusted contacts write to that number.
   Answers to a request the user made from the phone (section 5.7) also go to the bot's number.

One server hosts several users (main design section 2), so each user's WhatsApp is their own
(Claude's call):

- In own-number mode, each user links their own account through Selflow, for example with a
  `selflow` command that shows the linking code in the terminal. The server keeps one gateway
  session per user, and the admin never sees or handles another user's session (main design
  section 7.3).
- In separate-number mode, each bot number belongs to one user. A number shared by the whole server
  is out of scope.
- The server knows whose message it is from the session it arrived on, so one person can be a
  trusted contact of several users. A mother whose two children use the same server only sets off
  the rules of the child she wrote to.

## 6. Where it runs

The feature lives in `selflow-server`. The server already holds each user's live enforcement state
and pushes changes to their devices (main design section 6.6), so the path is short:

1. The WhatsApp gateway receives a message and calls a webhook on `selflow-server`.
2. The server finds the user from the gateway session the message arrived on (section 5.8) and the
   trusted contact from the sender's number. It checks the contact's windows and matches the text
   against the user's rules with the `selflow-core` logic the daemon uses.
3. On a match, the server starts the rule's steps and pushes them to the user's devices. Main design
   section 6.5 applies, so a lock from a trusted contact follows the user to every machine.
4. If the rule has replies on, the server sends the reply through the gateway.

A request from the lock screen goes the other way: daemon, then server, then gateway.

The server reaches the gateway through an adapter, with one implementation per provider. The owner
runs Evolution API and a second WhatsApp API on the Pi. Nobody has looked at either one yet
(section 10), and the builder picks which one the first adapter targets.

When no device can be reached (Claude's call): a step set off by a contact's message expires if none
of the user's devices receives it within a few minutes, because a lunch lock that arrives at six in
the evening helps nobody. The builder picks the length. With replies on, the contact is told the
message didn't reach the user.

## 7. Data

The project-wide rule in main design section 2 applies:

- The server drops messages from anyone who isn't a trusted contact as soon as the webhook sees
  them, without logging, storing or matching them.
- The server matches messages from trusted contacts and then drops them. Their text is never
  stored, on the server or on a device.
- The event log keeps what happened and who caused it: "the rule *Lunch* was set off by Mom at
  12:30", "Mom granted 20 minutes", "refused outside Mom's window". The next-day report reads from
  it.
- Selflow sends a request written on the lock screen and then drops it. The request isn't a written
  reason (main design section 8.5, challenge 2), so it isn't kept. (Claude's call)
- The configuration keeps each contact's name, phone number and windows, because the feature can't
  work without them.
- The gateway's credentials are a server secret and get the same handling as the other server
  secrets.
- The rule covers the gateway too, because in own-number mode it sits on the user's whole WhatsApp
  account. The gateway Selflow uses runs with message, chat and contact storage off. If the gateways
  already on the Pi serve other things that need storage, Selflow gets its own instance.
  (Claude's call, following the owner's rule)

## 8. Changing the configuration

Main design section 8.9 applies. This is how the new settings sort out (Claude's call):

| Change | Counts as |
|---|---|
| Adding a trusted contact with no rules | neutral |
| Adding a rule a contact can set off that has a lock, a shutdown, a block or an agent step | stricter |
| Widening a contact's windows | stricter |
| Removing a contact, narrowing their windows, or removing a rule they can set off | loosening |
| Letting the user end a contact's lock alone, or letting the contact fully unlock | loosening |
| Turning replies on or off, and changing reply texts | neutral |
| Switching between own-number and separate-number mode | loosening, because contacts keep writing to the old number until they learn the new one |

When it's unclear whether a change loosens, it counts as loosening, as main design section 8.9
says.

## 9. Feature flag

Trusted contacts are an optional area, and a user who doesn't want them never sees them. How flags
work, including whether switching an area off counts as loosening, belongs to the feature flags
design (main design section 21).

## 10. Open questions
- What runs on the Pi. The owner has Evolution API and a second WhatsApp API there, and nobody has
  looked at either yet, because the Pi's SSH key needs a passphrase that wasn't loaded in the agent
  (main design section 14). Before the first adapter is planned, check what each one offers for
  incoming webhooks, sending replies, one session per user (section 5.8) and running without
  storing messages (section 7), and whether other services already use them. The builder check
  read in Evolution API's documentation that its storage settings apply to the whole instance
  rather than to each session. That still has to be confirmed against the version on the Pi.
- Whether linking a personal number to an unofficial gateway is safe. Own-number mode needs the
  gateway to act as a linked WhatsApp device. Check WhatsApp's terms and the gateway's guidance on
  the risk to the number before relying on that mode.

## 11. Left to the builder

- The configuration format for trusted contacts, message conditions and the new rule settings,
  within main design section 20.
- The provider the first adapter targets, and the adapter's interface.
- How text is normalized for word lists (accents, case, punctuation) and which regular expression
  engine runs the patterns.
- The exact preset lists in both languages.
- How long a step set off by a contact's message waits for a device before it expires (section 6).
- The default reply texts in both languages.
