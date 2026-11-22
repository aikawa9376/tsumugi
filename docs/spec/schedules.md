# Schedules

A schedule asks an agent for the same work again and again. The work host
keeps it and, each time it is due, queues its prompt as a local issue in the
schedule's working directory, so the request waits its turn and runs as any
other local issue does ([local issues](local-issues.md)). A schedule can instead
wake a session that is already running ([waking a session](#waking-a-session)).
Closing a window does not stop a schedule; the host runs it for as long as it
runs.

## Making and allowing

A schedule has a name, a prompt, a working directory, an agent profile (the
default when none is named), a time zone, a cadence and a grace for missed
times. The cadence is hourly at a minute, daily, weekdays, or weekly on chosen
days, at a time of day read on the clock of the time zone, or every so many
minutes, from 5 minutes to 24 hours, counted from the Unix epoch so that the
times are the same in every zone. Cron expressions, RRULEs, retries and checks
before a run are not read.

A schedule is always made paused. Enabling it allows its times from that moment
on; a time that passed before it was enabled, or while it was paused, never
runs. Saving a local issue never gives it a schedule, and a schedule never
starts anything by itself while it is paused. Changing a schedule's cadence or
time zone allows its times from that moment on, as enabling does.

Each schedule shows whether it is enabled, when it is next due, and how the
last time it was due ended. They are shown in the Local Issues sheet and, from
a terminal, by `tsumugi schedule`. An issue a schedule queued says so.

## When a time is due

- A time is run once. A host that restarts, or two that look at the same
  moment, queue it once.
- A time that passed while the host was off runs once, as its latest time, when
  it is within the schedule's grace (12 hours unless set); otherwise it is
  recorded as skipped. Earlier missed times are not run one by one.
- A clock that skips an hour runs a time in it when the clock next reads it; a
  clock that repeats an hour runs a time in it the first time round. A daily
  schedule runs once a day, and an hourly one once an hour, whatever the clock
  does.
- A schedule keeps at most one request unfinished in the queue. While its last
  request is queued, working or waiting on the user, the next time is recorded
  as skipped and adds nothing.
- A time that cannot run says what to put right, and the next time is tried as
  usual. An unavailable directory and a profile that is no longer defined are
  such failures; a profile deleted after a request was queued fails that issue
  as any local issue fails.

Changing or deleting a schedule leaves the issues it queued alone, including
work in progress. The last fifty times a schedule was due are kept with it.

## Waking a session

A schedule made for a session hands its prompt to that session, in place of
queueing a request, as a message the session reads between its turns. It is
how a supervisor is checked on at a long interval, as a safeguard beside the
reports its workers send ([supervised workers](opening-sessions.md#supervised-workers)):
a worker that stops wakes its supervisor at once, and the tick only covers
what no report told.

- A tick is handed over only while the session waits for input. Times that
  come due while it works are held, however many, as one tick, which is handed
  over when its turn ends; no tick is added while the last has not reached the
  session. A session is never given two ticks to answer.
- A held tick is kept by the work host: it survives the host restarting and the
  windows closing, and is handed over once.
- A schedule that wakes a session needs no directory or profile, and none of
  its times is queued as an issue.
- When the session ends, the schedule pauses itself and says why. A session
  resumed keeps its code, and the schedule can be enabled again.
- Pausing a schedule, enabling it or changing its timetable drops a tick it
  holds, as it allows the times from that moment on. Deleting a schedule, or
  pausing it, leaves the session and every worker it opened alone.
- `run` holds a tick now, enabled or not, and refuses while one is held.

A schedule shows whom it wakes and, while it holds one, since when.

`tsumugi schedule` works as `tsumugi issue` does, with `--json` and
`--instance`, and follows Orca's `automations` in its words.

```
tsumugi schedule list
tsumugi schedule show <schedule>
tsumugi schedule create --name <name> --prompt <text|-> --trigger <hourly|daily|weekdays|weekly|every>
    [--time HH:MM] [--days mon,thu] [--timezone <zone>] [--dir <path>]
    [--profile <name>] [--missed-run-grace-minutes <n>] [--disabled]
    [--every-minutes <n>] [--session <code>]
tsumugi schedule edit <schedule> [the same options] [--default-profile] [--enabled|--disabled]
tsumugi schedule run <schedule>
tsumugi schedule runs <schedule>
tsumugi schedule remove <schedule>
```

A schedule is named by its number or the start of its id. `run` queues the
request now, enabled or not, and is refused while the last request is
unfinished. `--session` names the session a new schedule wakes by its code, which has to run, and without `--prompt` the tick carries a short standing request to look at how the sessions it opened stand. `--every-minutes` goes with `--trigger every`. Without `--timezone` the time zone of the work machine is used.
A failed command exits 1 and a command not understood exits 2.
