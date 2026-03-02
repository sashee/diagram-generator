{ pkgs }:
let
  rejects = builtins.toFile "fontconfig-profile-a-reject.conf" ''<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <match target="pattern">
    <test qual="all" name="family" compare="not_eq">
      <string>Noto Sans</string>
    </test>
    <edit name="family" mode="append_last">
      <string>Noto Sans</string>
    </edit>
  </match>
</fontconfig>
'';
  cache = pkgs.makeFontsCache {
    fontDirectories = [ pkgs.noto-fonts ];
  };
in
pkgs.writeTextFile {
  name = "fontconfig-profile-a.xml";
  text = ''<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <reset-dirs />
  <include>${rejects}</include>
  <cachedir>${cache}</cachedir>
  <dir>${pkgs.noto-fonts}</dir>

  <alias>
    <family>sans-serif</family>
    <prefer>
      <family>Noto Sans</family>
    </prefer>
  </alias>

  <alias>
    <family>serif</family>
    <prefer>
      <family>Noto Serif</family>
    </prefer>
  </alias>

  <alias>
    <family>monospace</family>
    <prefer>
      <family>Noto Sans Mono</family>
    </prefer>
  </alias>
</fontconfig>
'';
}
