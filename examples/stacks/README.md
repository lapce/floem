In this example, it shows the ability to have 1 million fixed height items in a list. 

What it does behind the scenes, is effectively only add the list item view on the screen to the view tree, and remove them from the view tree when they are out of view.

The ```VirtualList``` in Floem gives the user a way to deal with really long lists in a performant way without manually doing the adding and removing.