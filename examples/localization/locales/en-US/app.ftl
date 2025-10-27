inc = increment
dec = decrement
val = { $counter ->
    [0] There is no value
    *[one] There is {$counter} value
    [other] They are {$counter} values
}