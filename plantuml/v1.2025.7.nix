let
	plantuml_1_2025_4 = import ./v1.2025.4.nix;
in
{
	version = "v1.2025.7";
	formats = [ "svg" "png" ];
	bin = let
		pkgs = plantuml_1_2025_4.pkgs;
		wrapper = plantuml_1_2025_4.makewrapper {
			inherit pkgs;
			src = pkgs.fetchurl {
				url = "https://github.com/plantuml/plantuml/releases/download/v1.2025.7/plantuml-1.2025.7.jar";
				hash = "sha256-TtzdoWSkvi+PlU+ChoeVUA69SfQjBiNr+I6sQffiF6g=";
			};
		};
	in
		wrapper;
}
