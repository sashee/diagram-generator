{pkgs}:
let
rejects = (builtins.toFile "reject.conf" ''<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <match target="pattern">
    <test qual="all" name="family" compare="not_eq">
      <string>sans-serif</string>
    </test>
    <test qual="all" name="family" compare="not_eq">
      <string>serif</string>
    </test>
    <test qual="all" name="family" compare="not_eq">
      <string>monospace</string>
    </test>
    <edit name="family" mode="append_last">
      <string>sans-serif</string>
    </edit>
  </match>

	<alias>
		<family>sans-serif</family>
		<prefer><family>Noto Sans</family></prefer>
	</alias>
	<alias>
		<family>monospace</family>
		<prefer><family>Noto Sans Mono</family></prefer>
	</alias>
	<alias>
		<family>serif</family>
		<prefer><family>Noto Serif</family></prefer> 
	</alias>
</fontconfig>
  '');
		cache = pkgs.makeFontsCache {
			fontDirectories = [
				pkgs.noto-fonts
			];
		};
		fontconfig = pkgs.writeTextFile{

		name = "aa";
		text = ''<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
<reset-dirs />
<include>${rejects}</include>
<cachedir>${cache}</cachedir>
<dir>${pkgs.noto-fonts}</dir>
</fontconfig>
		'';
		};
in
	fontconfig
