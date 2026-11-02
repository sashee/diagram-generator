let
	nixpkgs_rev = "fa1373130d5a21c040a47c5dbe1913d59ed91808";
	nixpkgs_src = fetchTarball {
		url = "https://github.com/NixOS/nixpkgs/archive/${nixpkgs_rev}.tar.gz";
		sha256 = "16x4xwxrp7aqrdd3k06rc94wnxh877incf2fscf03q6qnb326f0z";
	};
	pkgs = import nixpkgs_src {};
	lib = pkgs.lib;

	version = "1.2026.9";
	commit = "18d0aae311a4f1eb3723bb4b0ad939185e22f166";
	# commit date of the release, used instead of the wall clock to keep the build reproducible
	commitTimestampMillis = "1792224379000";

	src = pkgs.fetchFromGitHub {
		owner = "plantuml";
		repo = "plantuml";
		rev = commit;
		hash = "sha256-zJyfQW/1cdf+4lwjly/SgdqQYkTwVQnCiW7NPbhAa/w=";
	};

	fetchMaven = { group, artifact, version, hash }: pkgs.fetchurl {
		url = "https://repo1.maven.org/maven2/${builtins.replaceStrings [ "." ] [ "/" ] group}/${artifact}/${version}/${artifact}-${version}.jar";
		inherit hash;
	};

	# runtimeClasspath of the upstream gradle build (gradle/libs.versions.toml + resolved transitive deps)
	runtimeDeps = map fetchMaven [
		{ group = "org.scilab.forge"; artifact = "jlatexmath"; version = "1.0.7"; hash = "sha256-bQTDhD/JjS7qm6TP8L57gFQMp1xaTgs0Y/0SGcSMxl0="; }
		{ group = "org.scilab.forge"; artifact = "jlatexmath-font-greek"; version = "1.0.7"; hash = "sha256-zdx+fm8+f/BDTIpIBercPp3IBFfVgtGcNpnmqu6PavI="; }
		{ group = "org.scilab.forge"; artifact = "jlatexmath-font-cyrillic"; version = "1.0.7"; hash = "sha256-GgFQ5L5k24jCUN/0H1kzSfVfDJz9uOOUM/Fbpz7NIbg="; }
		{ group = "org.eclipse.elk"; artifact = "org.eclipse.elk.core"; version = "0.12.0"; hash = "sha256-1W3DzGUMHzRmdfIYDmTv7UWNJkriIexnfmD27cdSZQI="; }
		{ group = "org.eclipse.elk"; artifact = "org.eclipse.elk.graph"; version = "0.12.0"; hash = "sha256-RLAmPWIGq5wtapbo21lqick08SI/q9LyCa2htzACIus="; }
		{ group = "org.eclipse.elk"; artifact = "org.eclipse.elk.alg.common"; version = "0.12.0"; hash = "sha256-5/6d2DjyqzOzDk2mObpxDETIc06cJbBtcVqRp4gOYXI="; }
		{ group = "org.eclipse.elk"; artifact = "org.eclipse.elk.alg.layered"; version = "0.12.0"; hash = "sha256-EXdNduSFXxgTNbphwmjpt0NPLd9ca4foLdHcUdbmgh0="; }
		{ group = "org.eclipse.elk"; artifact = "org.eclipse.elk.alg.mrtree"; version = "0.12.0"; hash = "sha256-CckcE3b1L3IEQIkaprJ1wSyEQ4sBOodl4g1ECd1kXhM="; }
		{ group = "org.eclipse.emf"; artifact = "org.eclipse.emf.common"; version = "2.12.0"; hash = "sha256-gLnd0BdLvFYjdn7Z9MlrF66Y/677jtvMeA1ZsNfv74M="; }
		{ group = "org.eclipse.emf"; artifact = "org.eclipse.emf.ecore"; version = "2.12.0"; hash = "sha256-KsaRzE+gn98Yvu9r3UV7e9/k0wH5cHqMicbLIJE59Pk="; }
		{ group = "org.eclipse.emf"; artifact = "org.eclipse.emf.ecore.xmi"; version = "2.12.0"; hash = "sha256-uMMF7nddiFoIklbFtoH2Tc54/IIdJxSiGUyJQEjAPhE="; }
		{ group = "org.eclipse.xtext"; artifact = "org.eclipse.xtext.xbase.lib"; version = "2.44.0"; hash = "sha256-xsK4WTzTnCeedOcFXL0UbTncbxYBGWG0kouSco0y0A0="; }
		{ group = "com.github.librepdf"; artifact = "openpdf"; version = "3.0.5"; hash = "sha256-KnDGAkuxC2GStLkdi7WB3iS3dqeZow71uHdI5UrKIUQ="; }
		{ group = "com.google.guava"; artifact = "guava"; version = "33.6.0-jre"; hash = "sha256-3Fc+H8pP1UVPSl/T19ot8DACh2pBdbr8FKlZgN13E7M="; }
		{ group = "com.google.guava"; artifact = "failureaccess"; version = "1.0.3"; hash = "sha256-y/w5BrGbj1XdfP1t/gqkUy6DQlDX8IC9jSEaPiRrWcs="; }
		{ group = "com.google.guava"; artifact = "listenablefuture"; version = "9999.0-empty-to-avoid-conflict-with-guava"; hash = "sha256-s3KgN9QjCqV/vv/e8w/WEj+cDC24XQrO0AyRuXTzP5k="; }
		{ group = "com.google.errorprone"; artifact = "error_prone_annotations"; version = "2.47.0"; hash = "sha256-U2S8byLnLpgZXkBqWNO6HAn/oR3qBylZLLhw3C3kBW0="; }
		{ group = "com.google.j2objc"; artifact = "j2objc-annotations"; version = "3.1"; hash = "sha256-hNOhUFGEhfgUDqmbiphWVnSWKfZDPJK4DHWzaro7CZs="; }
		{ group = "org.jspecify"; artifact = "jspecify"; version = "1.0.0"; hash = "sha256-H61ua+dVd4Hk0zcp1Jrhzcj92m/kd7sMxozjUer9+6s="; }
		{ group = "com.aayushatharva.brotli4j"; artifact = "brotli4j"; version = "1.23.0"; hash = "sha256-X88TrXjQcxnMUHRl3PYcvD7+6fRcmuHEKpkCQUaWvAs="; }
		{ group = "com.aayushatharva.brotli4j"; artifact = "service"; version = "1.23.0"; hash = "sha256-fymNRNu3aIQZ8Bee4d+g5ENmMycrBdOKX686K40IaXs="; }
	];

	# only needed by javac: the ant task and the TeaVM (JS) entry points, upstream does not bundle them
	compileOnlyDeps = map fetchMaven [
		{ group = "org.apache.ant"; artifact = "ant"; version = "1.10.18"; hash = "sha256-oS13X1ga2HWSsbHH/2+lvPa7eEl0uUyEiwaVrhV2X0Y="; }
		{ group = "org.apache.ant"; artifact = "ant-launcher"; version = "1.10.18"; hash = "sha256-uW5h7EWpy7LPjQCcR8pUS0UfQZ3xBRlakZIFr3r0eqI="; }
		{ group = "org.teavm"; artifact = "teavm-jso-apis"; version = "0.16.0"; hash = "sha256-MtKOOZV6QUBPU6FMJEfZgMe1B0qSQU0JcgFPRIpG/LI="; }
		{ group = "org.teavm"; artifact = "teavm-jso"; version = "0.16.0"; hash = "sha256-WLM3uTkkEGv4GcRYkMfCMaDHQBRa/nd5Xx4PCZ9iHXE="; }
		{ group = "org.teavm"; artifact = "teavm-interop"; version = "0.16.0"; hash = "sha256-XIUR1xrm9tLKkTEwOTtkqVkwPlIXJ7LZwamx4UUZIW8="; }
	];

	graalvm = pkgs.graalvmPackages.graalvm-ce;
	aotFontconfig = import ./aot-fontconfig.nix { inherit pkgs; };
	# since 1.2026.0, DOT diagrams are only rendered as SVG with INSECURE or INTERNET_WITH_DOTSVG (upstream issue #2495).
	# Compared to SANDBOX, INTERNET_WITH_DOTSVG also tries http(s) URLs, but sandbox-run blocks the network, and reads
	# local files only from allowlists, which the wrapper clears. The upstream issue is about javascript: links in the
	# SVG Graphviz generates: the font inliner rebuilds the drawing, which drops links (tests/contract-dot-svg-links.test.mjs).
	# The children of <defs> (except <style>) are copied unchanged though, e.g. the <script> of !pragma svginteractive;
	# Graphviz only writes gradients it generates there
	securityProfile = "INTERNET_WITH_DOTSVG";

	# equivalent of the upstream `patchCompilationInfo` + `jar` gradle tasks
	jar = pkgs.stdenv.mkDerivation {
		pname = "plantuml-jar";
		inherit version src;

		# stripJavaArchivesHook normalizes the timestamps and the order of the entries in the jar
		nativeBuildInputs = [ graalvm pkgs.unzip pkgs.stripJavaArchivesHook ];

		buildPhase = ''
runHook preBuild

substituteInPlace src/main/java/net/sourceforge/plantuml/version/CompilationInfo.java \
	--replace-fail '$version$' '${version}' \
	--replace-fail '$git.commit.id$' '${builtins.substring 0 7 commit}' \
	--replace-fail 'COMPILE_TIMESTAMP = 000L' 'COMPILE_TIMESTAMP = ${commitTimestampMillis}L'

find src/main/java -name '*.java' > sources.txt
javac -J-Xmx4g -nowarn -encoding UTF-8 --release 11 \
	-cp ${lib.concatStringsSep ":" (runtimeDeps ++ compileOnlyDeps)} \
	-d classes @sources.txt
cp -r src/main/resources/. classes/

# fat jar like upstream: project files win over dependency files, signatures are dropped
mkdir services
for dep in ${toString runtimeDeps}; do
	unzip -qq -n "$dep" -d classes
	unzip -qq "$dep" 'META-INF/services/*' -d "services/$(basename "$dep")" || [ $? -eq 11 ]
done
rm -f classes/META-INF/MANIFEST.MF classes/META-INF/*.SF classes/META-INF/*.DSA classes/META-INF/*.RSA

# unlike upstream (where the first one wins), merge the service registrations of the dependencies like a plain classpath
# would: several ELK jars register layout algorithms, and with the first one only ELK misses its default algorithm
rm -rf classes/META-INF/services
mkdir -p classes/META-INF/services
for service in services/*/META-INF/services/*; do
	cat "$service" >> "classes/META-INF/services/''${service##*/}"
	echo >> "classes/META-INF/services/''${service##*/}"
done

printf 'Main-Class: net.sourceforge.plantuml.Run\nImplementation-Version: ${version}\n' > manifest.txt
jar --create --file plantuml.jar --manifest manifest.txt -C classes .

runHook postBuild
		'';

		installPhase = ''
runHook preInstall
install -Dm644 plantuml.jar -t $out/share/plantuml
runHook postInstall
		'';
	};

	# equivalent of the upstream native-agent workflow: run the jar under the tracing agent to collect the
	# JNI/reflection metadata static analysis misses. The metadata shipped in the jar is traced on Windows (and
	# upstream builds with Liberica NIK, which has its own AWT metadata), so it is not enough for GraalVM CE on Linux
	metadata = pkgs.runCommand "plantuml-native-image-metadata-${version}" {
		nativeBuildInputs = [ graalvm ];
	} ''
export FONTCONFIG_FILE=${aotFontconfig}
# the GraalVM java wrapper does not make the dlopen-ed fontconfig findable
export LD_LIBRARY_PATH=${lib.getLib pkgs.fontconfig}/lib
export GRAPHVIZ_DOT=${pkgs.graphviz}/bin/dot
export JAVA_TOOL_OPTIONS="-Djava.io.tmpdir=$TMPDIR"

config=$out/META-INF/native-image/plantuml-trace
bash ${src}/.github/scripts/native-agent-trace.sh ${jar}/share/plantuml/plantuml.jar $config > /dev/null

# features whose code paths the metadata above misses (AWT image operations, reflection in ELK and jlatexmath, ...),
# rendered the way diagram-generator calls PlantUML. Every diagram has to render without errors
for format in svg png; do
	mkdir $format
	cp ${./native-trace}/*.puml $format/
	(cd $format && PLANTUML_SECURITY_PROFILE=${securityProfile} java -Djava.awt.headless=true -agentlib:native-image-agent=config-merge-dir=$config -jar ${jar}/share/plantuml/plantuml.jar . -t$format -o out -nometadata)
done
	'';

	native = pkgs.buildGraalvmNativeImage {
		pname = "plantuml-native";
		inherit version;
		src = "${jar}/share/plantuml/plantuml.jar";
		executable = "plantuml";
		graalvmDrv = graalvm;

		# same as upstream plantuml-natif/build.gradle.kts, -march=compatibility is a default of buildGraalvmNativeImage
		extraNativeImageBuildArgs = [
			"-Djava.awt.headless=true"
			"--enable-url-protocols=https"
			"--initialize-at-run-time=net.sourceforge.plantuml.brotli.Dictionary,net.sourceforge.plantuml.brotli.Dictionary$DataLoader,net.sourceforge.plantuml.brotli.DictionaryData"
		];

		buildPhase = ''
runHook preBuild
native-image -cp "$src:${metadata}" net.sourceforge.plantuml.Run ''${nativeImageArgs[@]}
runHook postBuild
		'';

		# the binary loads the AWT JNI libraries from its own directory.
		# $out is used as java.home (AWT requires it): an existing lib dir without a fontconfig.properties in it
		# makes AWT configure the fonts through fontconfig
		installPhase = ''
runHook preInstall
install -Dm755 plantuml *.so -t $out/bin
mkdir -p $out/lib
runHook postInstall
		'';

		# the AWT libraries are copied from the GraalVM JDK and still point to it: resolve libjava/libjvm to the
		# shims generated next to them instead, and make the dlopen-ed fontconfig findable
		# (after the fixup phase, as shrinking the RPATH would drop fontconfig)
		postFixup = ''
for so in $out/bin/*.so; do
	rpath=$(patchelf --print-rpath "$so" | tr ':' '\n' | sed '\|^${graalvm}|d' | paste -sd: -)
	patchelf --set-rpath "\$ORIGIN:${lib.getLib pkgs.fontconfig}/lib''${rpath:+:$rpath}" "$so"
done
		'';
	};

	wrapper = pkgs.writeShellScriptBin "plantuml" ''
: "''${TMPDIR:=/tmp}"
export PATH="${
	lib.makeBinPath [
		pkgs.fontconfig
		pkgs.coreutils
	]
}"
export GRAPHVIZ_DOT="${pkgs.graphviz}/bin/dot"
export PLANTUML_SECURITY_PROFILE="${securityProfile}"

# PlantUML reads the allowlists from the environment too (as property name and in upper case)
exec ${pkgs.coreutils}/bin/env \
	-u plantuml.include.path -u PLANTUML_INCLUDE_PATH \
	-u plantuml.allowlist.path -u PLANTUML_ALLOWLIST_PATH \
	-u plantuml.allowlist.url -u PLANTUML_ALLOWLIST_URL \
	${native}/bin/plantuml -Djava.home=${native} -Djava.io.tmpdir="$TMPDIR" -Djava.awt.headless=true "$@"
	'';
in
{
	version = "v${version}";
	formats = [ "svg" "png" ];
	inherit pkgs jar metadata native;
	bin = "${wrapper}/bin/plantuml";
}
