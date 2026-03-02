let
	supported_versions = builtins.fromJSON (builtins.readFile ./supported-versions.json);
in {
	inherit supported_versions;

	svg_to_png = {
		pkgs,
		debug ? false,
	}: import ./svg-to-png.nix {
		inherit debug pkgs;
	};

	svg_font_inliner = {
		pkgs,
		fontconfig,
		debug ? false,
	}: import ./svg-font-inliner.nix {
		inherit debug fontconfig pkgs;
	};

	diagram_generator = {
		pkgs,
		fontconfig,
		versions,
		debug ? false,
	}:
		let
			sandbox_run = import ./sandbox-run.nix { inherit debug pkgs; };

			available_renderers = {
				"plantuml" = (map
					({version, formats}: {
						bin = (import (./plantuml + "/${version}.nix")).bin {inherit version fontconfig;};
						version = version;
						formats = formats;
						renderer = ''plantuml-${version}'';
					})
					versions.plantuml
				);
				"recharts" = (map
					({version, formats}: {
						bin = (import (./recharts + "/${version}.nix")).bin {version = version;};
						version = version;
						formats = formats;
						renderer = ''recharts-${version}'';
					})
					versions.recharts
				);
				"swirly" = (map
					({version, formats}: {
						bin = (import (./swirly + "/${version}.nix")).bin {version = version;};
						version = version;
						formats = formats;
						renderer = ''swirly-${version}'';
					})
					versions.swirly
				);
			};

			validated_available_renderers = if pkgs.lib.allUnique
			(builtins.map
				(config: config.renderer)
				(builtins.concatLists (builtins.attrValues available_renderers))
			)
			then available_renderers
			else throw "renderer strings not unique";
		in
		(import ./diagram-generator.nix {
			inherit debug fontconfig pkgs sandbox_run validated_available_renderers;
		}).bin;
}
