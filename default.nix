{
	plantuml_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).plantuml,
	recharts_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).recharts,
	swirly_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).swirly,
	debug ? false,
	fontconfig,
	pkgs,
}:
let
	sandbox_run = import ./sandbox-run.nix { inherit debug pkgs; };
	svg_font_inliner = import ./svg-font-inliner.nix {
		inherit debug fontconfig pkgs;
	};
	svg_to_png = import ./svg-to-png.nix {
		inherit debug pkgs;
	};

	available_renderers = {
		"plantuml" = (map
			({version, formats}: {
				bin = (import (./plantuml + "/${version}.nix")).bin {inherit version fontconfig;};
				version = version;
				formats = formats;
				renderer = ''plantuml-${version}'';
			})
			plantuml_versions
		);
		"recharts" = (map
			({version, formats}: {
				bin = (import (./recharts + "/${version}.nix")).bin {version = version;};
				version = version;
				formats = formats;
				renderer = ''recharts-${version}'';
			})
			recharts_versions
		);
		"swirly" = (map
			({version, formats}: {
				bin = (import (./swirly + "/${version}.nix")).bin {version = version;};
				version = version;
				formats = formats;
				renderer = ''swirly-${version}'';
			})
			swirly_versions
		);
	};

	validated_available_renderers = if pkgs.lib.allUnique
	(builtins.map
		(config: config.renderer)
		(builtins.concatLists (builtins.attrValues available_renderers))
	)
	then available_renderers
	else throw "renderer strings not unique";

	diagram_generator = import ./diagram-generator.nix {
		inherit debug fontconfig pkgs sandbox_run validated_available_renderers;
	};

	bin = diagram_generator.bin;
in {
	inherit bin;
	inherit sandbox_run;
	inherit svg_font_inliner;
	inherit svg_to_png;
}
