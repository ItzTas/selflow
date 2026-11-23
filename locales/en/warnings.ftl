notice-title = { $label }, at { $time }
notice-shutdown-title = { $label }: the computer shuts down at { $time }
notice-shutdown-now = The computer shuts down in a minute, at { $time }. A good moment to save what's open.
warning-body-1 = { $minutes ->
    [0] The screen locks now, at { $time }.
    [one] The screen locks in a minute, at { $time }.
   *[other] The screen locks in { $minutes } minutes, at { $time }.
}
warning-body-2 = { $minutes ->
    [0] The lock starts now, at { $time }.
    [one] One minute until the lock at { $time }. A good moment to save what's open.
   *[other] { $minutes } minutes until the lock at { $time }. A good moment to start wrapping up.
}
warning-body-3 = { $minutes ->
    [0] It's { $time }, and the screen locks now.
    [one] At { $time } the screen locks, about a minute from now. Save what you have open.
   *[other] At { $time } the screen locks, about { $minutes } minutes from now. You can start winding down.
}
notice-shutdown-body-1 = { $minutes ->
    [0] The computer shuts down now, at { $time }.
    [one] The computer shuts down in a minute, at { $time }.
   *[other] The computer shuts down in { $minutes } minutes, at { $time }.
}
notice-shutdown-body-2 = { $minutes ->
    [0] It's { $time }. The computer is shutting down now.
    [one] One minute until the computer shuts down at { $time }. A good moment to save what's open.
   *[other] { $minutes } minutes until the computer shuts down at { $time }. A good moment to start wrapping up.
}
notice-shutdown-body-3 = { $minutes ->
    [0] The computer shuts down now, as planned for { $time }.
    [one] At { $time } the computer shuts down, about a minute from now. Save what you have open.
   *[other] At { $time } the computer shuts down, about { $minutes } minutes from now. You can start winding down.
}
notice-reminder-body-1 = { $minutes ->
    [0] It's { $time }.
    [one] In a minute, at { $time }.
   *[other] In { $minutes } minutes, at { $time }.
}
notice-reminder-body-2 = { $minutes ->
    [0] It's { $time }, the time you set for this.
    [one] One minute to go, until { $time }.
   *[other] { $minutes } minutes to go, until { $time }.
}
notice-reminder-body-3 = { $minutes ->
    [0] { $time } is here. A good moment for a short pause.
    [one] At { $time }, about a minute from now.
   *[other] At { $time }, about { $minutes } minutes from now.
}
