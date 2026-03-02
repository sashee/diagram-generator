{ pkgs }:
let
	cache = pkgs.makeFontsCache {
		fontDirectories = [ pkgs.noto-fonts ];
	};
	fontconfig = pkgs.writeTextFile {
		name = "plantuml-aot-fontconfig.xml";
		text = ''<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <reset-dirs />
  <cachedir>${cache}</cachedir>
  <dir>${pkgs.noto-fonts}</dir>

  <alias>
    <family>sans-serif</family>
    <prefer>
      <family>Noto Sans</family>
    </prefer>
  </alias>

  <alias>
    <family>monospace</family>
    <prefer>
      <family>Noto Sans Mono</family>
    </prefer>
  </alias>

  <alias>
    <family>serif</family>
    <prefer>
      <family>Noto Serif</family>
    </prefer>
  </alias>
</fontconfig>
		'';
	};
in
	fontconfig
