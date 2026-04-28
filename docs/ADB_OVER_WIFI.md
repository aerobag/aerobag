# On laptop:

Plug in a USB cord and do the developer connection ritual.

`adb devices`
`adb tcpip 5555`

Then unplug.

# On dev container:

Look up `TABLET_IP` in Settings -> System -> About Tablet.

`adb connect <TABLET_IP>:5555`
