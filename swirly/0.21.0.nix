let
	plantuml_1_2025_4 = import ../plantuml/v1.2025.4.nix;
	recharts_2_15_4 = import ../recharts/2.15.4.nix;
in
{
	version = "0.21.0";
	formats = [ "svg" ];
	bin = let
		wrapper = recharts_2_15_4.makewrapper {
			pkgs = plantuml_1_2025_4.pkgs;
			packageDir = ./0.21.0;
			wrapperScript = ./0.21.0/index.ts;
		};
	in
		wrapper;
}
