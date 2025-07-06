let
	makewrapper = {pkgs, src}:
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
    <edit name="family" mode="append_first">
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

			wrapper = pkgs.writeShellScriptBin "plantuml" ''
export FONTCONFIG_FILE=${fontconfig}
export PATH="${
	pkgs.lib.makeBinPath [
		pkgs.fontconfig
		pkgs.coreutils
	]
}"

export GRAPHVIZ_DOT="${pkgs.graphviz}/bin/dot";
echo $FONTCONFIG_FILE
${pkgs.fontconfig}/bin/fc-match Serif

export JAVA_TOOL_OPTIONS="-XX:+SuppressFatalErrorMessage -Djava.io.tmpdir=/tmp -Djava.awt.headless=true"
export PLANTUML_SECURITY_PROFILE="SANDBOX"

#${pkgs.strace}/bin/strace -f -o /tmp/strace.log \
${pkgs.landrun}/bin/landrun --env FONTCONFIG_FILE --env JAVA_TOOL_OPTIONS --env PLANTUML_LIMIT_SIZE --env GRAPHVIZ_DOT --env PLANTUML_SECURITY_PROFILE \
--env PATH --rox /nix --rwx /tmp \
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
