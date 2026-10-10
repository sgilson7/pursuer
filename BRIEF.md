# Brief

Stage 1, written by the person. `C1 Frame the problem and set the rules`, `B1 Establish context`.

## The request, word for word (2026-10-10)

> Please use the Builder repo as a base in the directory above, and use the work and repos of Sam gilson at sgilson7.github.io, which can all be found one directory up. I want to make a 3d 1st person driving game in the style of something like crazy taxi but its a survival game. The game starts with you in a house walking around, and there are a bunch of objects to inspect. if you grab one and hold onto it for too long, the house falls away like a paper mario house, and a flat, still image of a celebrity of character related to the item that was inspected appears, and begins to chase you through a large, physics simulated city. as the giant still frame monster chases you, you are able to run to a car in the house, get in, then start trying to escape in 1st person, the thought is that you can essentially always see the monster in one of the rear view mirrors at any time. as the monster chases you as you drive away, it destroys the city, so bridges begin to fall apart, the buildings it passes through get destroyed and explode into pieces, im expecting like battlefield bad company levels of destructablity. based on the longer you survive, your car gets upgrades to deal with the destruction, like an ability to hover over debris for 3 seconds, to bigger stronger faster rotating wheels to get you out of a rut, to being able to fly the car like a plane for 5 seconds, and ther is no fall damage, and there is a button to slowly flip the car back over like the button in sgilson7 vagracy to sticky feet stand up. the game is a survival game and you are scored for how long you are able to survive. for music, the game should be silent besides foot squeaking noises as you walk around the house and pick up objects to inspect them, and then when the monster appears, it should play the song "are you coming to the tree" from the hunger games soundtrack. I want the physics and 3d design to come from iron vector and food truck circus and long time coming and the other examples like battlefield that i gave. I want the MVP built with one large about 1km wide city with an assortment of skyscrapers to short residential buildings, and the house you start in is smack dab int he middle of the city. the destructability should work and NOT lag the game out insanely bad, the game should be playable with the entire city reduced to their smallest component physics pieces all rendered sepeartely from each other. you can use example cities like new york or seattle to help you design the city layout. there should be 1 overpass, 5 hills, a downtown district, and a suburban district. please build out the MVP and deploy it to my website. you have approval for creating a public github repo, github pages, and approval to deploy to my website when its ready. any time you need to make 2D art and any kind of asset for the game, use the tikz latex prompt you can find in other repos, and use sam comm final as an inspiration image, alongside the menu screens of all of the sgilson7 games which use a similar build pipeline for images. now build the mvp for the game and deploy it, dont stop till its deployed, good luck. the game is called Pursuer

## The tool

**Pursuer**: a single-player first-person survival driving game for a desktop browser (keyboard and mouse).

## Done means

- The page loads from `https://sgilson7.github.io/pursuer/` with no off-origin request.
- You start inside a house in the middle of a 1 km city and walk in first person. Objects can be picked up and inspected; footsteps squeak.
- Holding an object too long folds the house flat and raises a giant flat cut-out of a character tied to that object, which chases you.
- A car in the house's garage can be entered and driven in first person, with rear-view and side mirrors that show the pursuer.
- The pursuer destroys what it passes through: buildings break into their component pieces, which fall under physics; the overpass loses its deck when its pillars go.
- The city has one overpass, five hills, a downtown and a suburb, with skyscrapers down to houses.
- The longer you survive, the more the car can do: bigger wheels, hover for 3 s, fly for 5 s. A held key slowly rolls the car upright. No fall damage.
- The score is the time survived; the best is kept.
- The frame rate stays playable with every piece of the city knocked loose.
- The game is listed on Sam's website.

## Decisions that stay with the person

- The song. "Are You Coming to the Tree" is a commercial recording; this build cannot publish it (SECOND-ORDER M1).
- The characters. The request says "a celebrity or character"; this build draws original characters only (SECOND-ORDER M2).
- Every number that tunes the feel.

## Autonomy

Sam approved in advance (2026-10-10): the plan, a public GitHub repository, GitHub Pages deploys and the website deploy. Where the brief is silent, the agent decides, records the decision in `SECOND-ORDER.md`, and continues. The agent may not loosen a Builder requirement or a boundary test to make work pass. It may not add a server, an account, analytics or any off-origin request.

## Allowed core dependencies

`serde`, `serde_json` (reference default). Nothing else.
