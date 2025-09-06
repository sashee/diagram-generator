let
	makewrapper = {pkgs, src, fontconfig}:
	let
		setup = ''
export FONTCONFIG_FILE=${fontconfig}
export PATH="${
	pkgs.lib.makeBinPath [
		pkgs.fontconfig
		pkgs.coreutils
	]
}"

export GRAPHVIZ_DOT="${pkgs.graphviz}/bin/dot";

export JAVA_TOOL_OPTIONS="-XX:+SuppressFatalErrorMessage -Djava.io.tmpdir=$TMPDIR -Djava.awt.headless=true -XX:TieredStopAtLevel=1 -XX:+UnlockExperimentalVMOptions -XX:+UseEpsilonGC"
export PLANTUML_SECURITY_PROFILE="SANDBOX"
: "''${TMPDIR:=/tmp}"
		'';

		jsa = pkgs.runCommand "jsa" {} ''
${setup}

mkdir -p $out

cat ${./aot_testfiles.txt} | ${pkgs.jdk24}/bin/java -XX:AOTMode=record -XX:AOTConfiguration=$out/app.aotconf -jar ${src} -tsvg -pipe > /dev/null
cat ${./aot_testfiles.txt} | ${pkgs.jdk24}/bin/java -XX:AOTMode=create -XX:AOTConfiguration=$out/app.aotconf -XX:AOTCache=$out/app.aot -jar ${src} -tsvg -pipe > /dev/null
		'';

			wrapper = pkgs.writeShellScriptBin "plantuml" ''
${setup}

${pkgs.landrun}/bin/landrun --env FONTCONFIG_FILE --env JAVA_TOOL_OPTIONS --env PLANTUML_LIMIT_SIZE --env GRAPHVIZ_DOT --env PLANTUML_SECURITY_PROFILE \
--env PATH --env TMPDIR --rox /nix --rwx $TMPDIR --rwx /dev/random \
${pkgs.jdk24}/bin/java -XX:AOTCache=${jsa}/app.aot -XX:AOTMode=on -jar ${src} "$@"
	'';

in
	''${wrapper}/bin/plantuml'';

in
{
	makewrapper = makewrapper;
	bin = {version, fontconfig}: let
		pkgs = import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05") {};
		wrapper = makewrapper {
			inherit pkgs fontconfig;
			src = pkgs.fetchurl {
				url = "https://github.com/plantuml/plantuml/releases/download/${version}/plantuml-${builtins.substring 1 (-1) version}.jar";
				hash = "sha256-JlGOFKOgQQDNdsDZbKstEXHzYVIhXt2XkKKNICaCAME=";
			};
		};
	in
	wrapper;
}
