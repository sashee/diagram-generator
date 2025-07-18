import {renderMarbleDiagram} from "@swirly/renderer-node";
import {parseMarbleDiagramSpecification} from "@swirly/parser";
import process from "node:process";
import { text } from "node:stream/consumers";

const renderSwirly = async (code: string) => {
	console.error(code);
	const diagramSpecification = parseMarbleDiagramSpecification(code);
	console.error(diagramSpecification);
	const res = renderMarbleDiagram(diagramSpecification, {});
	console.error(res);
	return res.xml;
}

const stdin = await text(process.stdin);
console.log(await renderSwirly(stdin));

