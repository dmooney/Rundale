const HANDWRITTEN_LONG_WAIT_MESSAGES = [
	'Reticulating spleens...',
	'Reading the manifest by candlelight. The first download is the long one.',
	'Stamping a one-time library card for the local storyteller...',
	'Persuading several billion parameters to form an orderly queue...',
	'Checking that every byte has brought a coat for the weather...',
	'Copying the grand book into the parish shelves. Future starts should be quicker.',
	'Boiling the kettle for the manifest clerk...',
	'Filing the table of contents under "probably important"...',
	'Waiting for Ollama to finish naming every parcel before the heavy lifting begins...',
	'Polishing the little brass plaque that says "one-time model download"...',
	'Retiring the old local copy with full honours...',
	'Explaining to the manifest that yes, we do need all of it...',
	'Finding shelf space for a very large book of tales...',
	'Counting bytes in parish order, which is like normal order but more ceremonial...',
	'Teaching the ledger to say "qwen" without looking nervous...',
	'Warming the good chair for the storyteller...',
	'Sorting model parcels by weight, mood, and likelihood of fitting through the door...',
	'Asking the manifest to stop being coy about the download size...',
	'Putting little boots on the tensors for the road ahead...',
	'Cross-checking the table of contents against the table of unreasonable expectations...',
	'This big fetch only needs doing once unless you force a fresh download again.',
	'Rubbing the sleep from the model weights...',
	'Untangling a heroic amount of string from the parish archive...',
	'Convincing the progress bar that patience is also a form of progress...',
	'Applying a respectable amount of folklore to the dependency chain...',
	'Waiting on the manifest office, which keeps rural hours...',
	'Measuring the bookcase twice and downloading once...',
	'Sharpening pencils for the first tale...',
	'Unfolding the map that says "large model lives over here"...',
	'Making sure the storyteller gets the whole book, not just the dramatic bits...',
	'Tidying the hearth while the download does the expensive part...',
	'Checking the parcel labels before the heavy crates arrive...',
	'Giving the local cache a stern but encouraging talk...',
	'Putting the one-time download notice somewhere nobody can pretend not to see it...',
	'Inviting the manifest to declare its intentions...',
	'Practising patience in a period-appropriate fashion...',
	'Stacking virtual turf beside the virtual fire...',
	'Teaching the bytes to take turns at the gate...',
	'Preparing a comfortable corner for a frankly enormous storyteller...',
	'Looking busy in case the manifest is watching...',
	'Reading the fine print, then the finer print, then the suspiciously tiny print...',
	'Keeping the player entertained while Ollama negotiates with the registry...',
	'Putting a ribbon in the ledger at "almost certainly still working"...',
	'Waiting for the manifest to finish its solemn procession...',
	'Greasing the hinges on the model cupboard...',
	'Labelling crates: words, notions, hunches, dramatic pauses...',
	'Checking that the download is not secretly three smaller downloads in a long coat...',
	'Preparing the welcome mat for the first real progress tick...',
	'Folding a napkin for the GPU...',
	'Whispering encouragement to the network stack...',
	'Writing "one-time setup" on a slate in large friendly letters...',
	'Giving the manifest a moment. It has a lot on its mind.',
	'Baking a progress pie and arguing about who gets the percentage slice...',
	'Making room in the cache for a story-heavy guest...',
	'Waiting for the registry to hand over the good parcels...',
	'Keeping the cottage lights on while the big download settles itself...',
	'Preparing a tasteful plaque: "Downloaded locally, runs locally"...',
	'Reticulating the backup spleens, just to be thorough...',
	'Nearly through the quiet part. The noisy progress numbers should return when Ollama starts the parcels.'
] as const;

const WAITING_ACTIONS = [
	'Cataloguing',
	'Blessing',
	'Sorting',
	'Stacking',
	'Labelling',
	'Unfolding',
	'Inspecting',
	'Warming',
	'Coaxing',
	'Counting',
	'Polishing',
	'Translating',
	'Inventorying',
	'Threading',
	'Tuning',
	'Filing',
	'Measuring',
	'Indexing',
	'Whispering to',
	'Making room for'
] as const;

const WAITING_OBJECTS = [
	'the manifest candles',
	'the parish byte ledger',
	'the very patient tensors',
	'the storyteller parcels',
	'the model bookplates',
	'the cache shelves',
	'the GPU hearth',
	'the registry paperwork',
	'the download timetable',
	'the local archive',
	'the table of contents',
	'the wax seals',
	'the progress ribbons',
	'the tale crates',
	'the semantic teacups',
	'the parameter satchels',
	'the inference boots',
	'the context window curtains',
	'the vector cupboard',
	'the checksum rosary',
	'the byte-sized sandwiches',
	'the model pantry',
	'the manifest clerk notes',
	'the one-time setup slate',
	'the Ollama parcel tags'
] as const;

const WAITING_ENDINGS = [
	'while the big one-time download gets settled.',
	'before the next real progress tick arrives.',
	'so future launches can skip this long errand.',
	'without disturbing the technical bits.',
	'while Ollama negotiates with the registry.',
	'and keeping the local cache respectable.',
	'in the traditional parish manner.',
	'with more confidence than evidence.',
	'while the storyteller finds the correct shelf.',
	'and leaving the model name pinned to the ledger.',
	'because even manifests deserve ceremony.',
	'until the parcels start reporting their weight.'
] as const;

const GENERATED_WAIT_MESSAGE_LIMIT = 540;

function buildLongWaitMessages() {
	const messages = new Set<string>(HANDWRITTEN_LONG_WAIT_MESSAGES);
	let generated = 0;

	outer:
	for (const action of WAITING_ACTIONS) {
		for (const object of WAITING_OBJECTS) {
			for (const ending of WAITING_ENDINGS) {
				messages.add(`${action} ${object} ${ending}`);
				generated += 1;
				if (generated >= GENERATED_WAIT_MESSAGE_LIMIT) {
					break outer;
				}
			}
		}
	}

	return [...messages];
}

export const LONG_WAIT_MESSAGES = buildLongWaitMessages();
