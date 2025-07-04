let
	makewrapper = {pkgs, src}:
	let
		FONTCONFIG_FILE = pkgs.makeFontsConf {
			fontDirectories = [
				pkgs.noto-fonts
			];
			includes = [
(builtins.toFile "reject.conf" ''<?xml version="1.0"?>
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
		<prefer><family>Arial</family></prefer>
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
  '')
			];
		};

			wrapper = pkgs.writeShellScriptBin "plantuml" ''
export FONTCONFIG_FILE=${FONTCONFIG_FILE}
export PATH="${
	pkgs.lib.makeBinPath [
		pkgs.fontconfig
		pkgs.coreutils
		pkgs.graphviz
	]
}"

export GRAPHVIZ_DOT="${pkgs.graphviz}/bin/dot";

${pkgs.jdk}/bin/java -jar ${src} "$@"
	'';

in
	''${wrapper}/bin/plantuml'';

in
{
	makewrapper = makewrapper;
	bin = {version}: let
		pkgs = import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05") {};
		wrapper = makewrapper {
			pkgs = pkgs;
			src = pkgs.fetchurl {
				url = "https://github.com/plantuml/plantuml/releases/download/${version}/plantuml-${builtins.substring 1 (-1) version}.jar";
				hash = "sha256-JlGOFKOgQQDNdsDZbKstEXHzYVIhXt2XkKKNICaCAME=";
			};
		};
	in
	wrapper;
}
