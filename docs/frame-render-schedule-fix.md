We need to do a massive simplification of frame rendering and presenting especially in the window
and app handles. Currently there are a ton of methods that do similar or exactly the same thing and
there are also many methods like "render_if" having any "if" in the name of a method is a huge code
smell. We should be using matklads advice of lifting if's and pushing for's down (I'm sure you are
familiar with that blog post). We need to remove the methods that do similar things and instead
focus on having single methods that do things, conditioning the calling of that function using if
conditions and separating conflicting concepts. So separate concepts are: paint/scene, present frame
(there should not be a "present_ready_frame" there should just be present frame where we pass in a
frame or soemthing. this is where we really need to work on the model and data flow). painting
should never call present and present should not call paint. We also need to do a similar cleanup in
the frame clock for removing duplicate methods, separating concepts and data flow and pushing if's
up.

We should think about this considering the event loop, update processing, how scheduling interacts
with the update loop, if we need to add other update types, if present should be an update type,
data flow, where does data live, how do we make invalid states unrepresentable. We should probably
update the framme schedule design doc.
