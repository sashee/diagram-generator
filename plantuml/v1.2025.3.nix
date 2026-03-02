let
	plantuml_1_2025_4 = import ./v1.2025.4.nix;
in
{
	bin = {
		version,
		fontconfig
	}:
	let
		pkgs = plantuml_1_2025_4.pkgs;
		wrapper = plantuml_1_2025_4.makewrapper {
			inherit pkgs fontconfig;
			src = pkgs.fetchurl {
				url = "https://github.com/plantuml/plantuml/releases/download/${version}/plantuml-${builtins.substring 1 (-1) version}.jar";
				hash = "sha256-g3t5Iv4wysEzb9V8WuzILwoGAyx325SaaYOppq5BcEo=";
			};
		};
	in
		wrapper;
}
